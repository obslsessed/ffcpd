#[macro_use]
extern crate log;
extern crate simplelog;

use std::{
    fs::{create_dir_all, read_to_string, File},
    io::Write,
};

use anyhow::Result;
use futures::{stream, StreamExt};
use inquire::{validator::Validation, MultiSelect, Select, Text};
use rand::{seq::IteratorRandom, thread_rng};
use reqwest::Client;
use scraper::{ElementRef, Html, Selector};
use simplelog::{CombinedLogger, Config, SharedLogger, TermLogger, WriteLogger};
use spinners::Spinner;
use strum::IntoEnumIterator;
use tokio;

const PARALLEL_REQUESTS: usize = 100;

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
    let html = read_to_string("index.html")?;
    let document = Html::parse_document(&html);
    let selector = Selector::parse("a").unwrap();
    let links = document.select(&selector);
    let is_image = |x: &ElementRef<'_>| x.inner_html().ends_with(".jpg");
    let is_full_res = |x: &ElementRef<'_>| !x.inner_html().contains("_4");
    let values = links
        .filter(is_image)
        .filter(is_full_res)
        .map(|x| x.inner_html())
        .collect::<Vec<String>>();

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
        scrape_esaba(values, save_path).await;
        println!("🐱 enjoy the cats");
    } else {
        let mut rng = thread_rng();
        let spinner = spinners::Spinners::iter().choose(&mut rng).unwrap();
        let mut sp = Spinner::new(spinner, "doing it...".into());
        scrape_esaba(values, save_path).await;
        sp.stop_and_persist("🐱", "enjoy the cats".into());
    }
    Ok(())
}

async fn scrape_esaba(images: Vec<String>, save_path: String) {
    let urls = images
        .iter()
        .map(|x| format!("https://blog.esaba.com/projects/catphotos/images/{x}"))
        .collect::<Vec<String>>();
    dbg!(&urls);
    let client = Client::new();
    let bodies = stream::iter(urls)
        .map(|url| {
            let client = client.clone();
            tokio::spawn(async move {
                info!("opening website");
                // unwrap because it shouldn't be possible to be none
                let resp = client.get(url).send().await.unwrap();
                resp.bytes().await.unwrap()
            })
        })
        .buffer_unordered(PARALLEL_REQUESTS);

    bodies
        .for_each(|b| async {
            match b {
                Ok(b) => {
                    let filename = (1..)
                        .find(|i| {
                            let file = format!("{save_path}{i}.jpg");
                            File::open(file).is_err()
                        })
                        .unwrap();
                    dbg!(&filename);
                    info!("saving image {filename}");
                    let path = format!("{save_path}{filename}.jpg");
                    let mut file = File::create(path).unwrap();
                    file.write(&b).unwrap();
                }
                Err(e) => error!("tokio error: {}", e),
            }
        })
        .await;
}
