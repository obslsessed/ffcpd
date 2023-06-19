#[macro_use]
extern crate log;
extern crate simplelog;

use std::{
    fs::{create_dir_all, File},
    io::Write,
};

use anyhow::{anyhow, Result};
use futures::{stream, StreamExt};
use inquire::{validator::Validation, CustomType, MultiSelect, Select, Text};
use rand::{seq::IteratorRandom, thread_rng};
use reqwest::Client;
use simplelog::{CombinedLogger, Config, SharedLogger, TermLogger, WriteLogger};
use spinners::Spinner;
use strum::IntoEnumIterator;
use tokio;

const PARALLEL_REQUESTS: usize = 10;

#[tokio::main]
async fn main() -> Result<()> {
    let choices = vec!["funny cat photos", "stuff on my cat"];
    let source = Select::new("source:", choices).prompt()?;

    if source == "funny cat photos" {
        funny_cat_photos().await?;
    } else {
        todo!();
    }
    Ok(())
}

async fn funny_cat_photos() -> Result<()> {
    let number_of_requests = CustomType::<usize>::new("amount:")
        .with_placeholder("10")
        .with_error_message("put a fucking number")
        .prompt()?;
    if number_of_requests == 0 {
        return Err(anyhow!("kill yourself"));
    }

    let validator = |input: &str| match create_dir_all(input) {
        Err(error) => Ok(Validation::Invalid(error.into())),
        Ok(_) => Ok(Validation::Valid),
    };
    let mut save_path = Text::new("save to:")
        .with_placeholder(".")
        .with_validator(validator)
        .prompt()?;
    let no_trailing_slash = save_path.chars().last() != Some('/');
    let is_not_empty = save_path.len() != 0;
    if no_trailing_slash && is_not_empty {
        save_path.push('/');
    }

    let options = vec!["terminal", "file"];
    let log_to = MultiSelect::new("log to:", options).prompt()?;
    let mut log_places: Vec<Box<(dyn SharedLogger + 'static)>> = vec![];
    let log_to_terminal = log_to.contains(&"terminal");
    let log_to_file = log_to.contains(&"file");
    if log_to_terminal {
        let term_logger = TermLogger::new(
            simplelog::LevelFilter::Info,
            Config::default(),
            simplelog::TerminalMode::Mixed,
            simplelog::ColorChoice::Auto,
        );
        log_places.push(term_logger);
    }

    if log_to_file {
        let validator = |input: &str| match input.chars().any(|char| char == '/') {
            true => Ok(Validation::Invalid("don't put a fucking slash".into())),
            false => Ok(Validation::Valid),
        };
        let mut name = Text::new("filename:")
            .with_validator(validator)
            .with_placeholder("cat.log")
            .prompt()?;
        if name == "" {
            name = "cat.log".into();
        }
        let path = format!("{save_path}{name}");
        let file_logger = WriteLogger::new(
            simplelog::LevelFilter::Info,
            Config::default(),
            File::create(path)?,
        );
        log_places.push(file_logger);
    }
    CombinedLogger::init(log_places)?;

    if log_to_terminal {
        scrape_esaba(number_of_requests, save_path).await;
        println!("🐱 enjoy the cats");
    } else {
        let mut rng = thread_rng();
        let spinner = spinners::Spinners::iter().choose(&mut rng).unwrap();
        let mut sp = Spinner::new(spinner, "doing it...".into());
        scrape_esaba(number_of_requests, save_path).await;
        sp.stop_and_persist("🐱", "enjoy the cats".into());
    }
    Ok(())
}

async fn scrape_esaba(number_of_requests: usize, save_path: String) {
    let urls = vec!["https://blog.esaba.com/projects/catphotos/catphotos.php"; number_of_requests];
    let client = Client::new();
    let bodies = stream::iter(urls)
        .map(|url| {
            let client = client.clone();
            tokio::spawn(async move {
                info!("opening website");
                let resp = client.get(url).send().await?;
                resp.text().await
            })
        })
        .buffer_unordered(PARALLEL_REQUESTS);

    bodies
        .for_each(|b| async {
            match b {
                Ok(Ok(b)) => {
                    let (_, half) = b.split_once("<img src=\"images/").unwrap();
                    let (name, _) = half.split_once("\" />").unwrap();
                    let url = format!("https://blog.esaba.com/projects/catphotos/images/{name}");
                    info!("opening image {name}");
                    let resp = client.get(url).send().await.unwrap();
                    let img = resp.bytes().await.unwrap();
                    match File::open(name) {
                        Ok(_) => error!("{name} already exists"),
                        Err(_) => {
                            info!("saving image {name}");
                            let path = format!("{save_path}{name}");
                            let mut file = File::create(path).unwrap();
                            file.write(&img).unwrap();
                        }
                    }
                }
                Ok(Err(e)) => error!("reqwest error: {}", e),
                Err(e) => error!("tokio error: {}", e),
            }
        })
        .await;
}
