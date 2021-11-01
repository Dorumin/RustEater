#![deny(clippy::all)]
#![deny(clippy::nursery)]
#![deny(clippy::pedantic)]

use std::time::{Duration, Instant};
use serde::{Serialize, Deserialize};
use structopt::StructOpt;
use async_std::task;

#[derive(Debug, StructOpt)]
#[structopt(name = "eater", about = "Eats Discord DMs, whole.")]
struct Options {
    #[structopt(long)]
    id: String,

    #[structopt(long)]
    token: String,

    #[structopt(long)]
    start: Option<String>,

    #[structopt(long)]
    target: Option<String>,

    #[structopt(long, use_delimiter = true)]
    exclude: Option<Vec<String>>
}

#[derive(Serialize, Deserialize)]
struct UserInfo {
    username: String,
    id: String,
}

async fn fetch_info(options: &Options) -> Result<UserInfo, reqwest::Error> {
    let client = reqwest::Client::new();
    let res = client.get("https://discord.com/api/v6/users/@me")
        .header("Authorization", &options.token)
        .send()
        .await?
        .json::<UserInfo>()
        .await?;

    Ok(res)
}

#[derive(Serialize, Deserialize)]
struct Message {
    r#type: i32,
    id: String,
    author: Author,
    attachments: Vec<Attachment>
}

#[derive(Serialize, Deserialize)]
struct Author {
    id: String
}

#[derive(Serialize, Deserialize)]
struct Attachment {
    id: String
}

#[derive(Serialize, Deserialize)]
struct RatelimitError {
    message: String,
    retry_after: u64
}

async fn fetch_messages(options: &Options) -> Result<Vec<Message>, reqwest::Error> {
    let client = reqwest::Client::new();
    let mut last_id = options.start.clone().unwrap();
    let mut messages = vec![];

    loop {
        let url = format!("https://discord.com/api/v6/channels/{}/messages?after={}&limit=100", &options.id, last_id);
        let res = client.get(&url)
            .header("Authorization", &options.token)
            .send()
            .await?;

        if res.status().as_u16() == 429 {
            task::sleep(Duration::from_millis(5000)).await;
            continue;
        }

        let mut res = res.json::<Vec<Message>>()
            .await?;

        if res.is_empty() {
            break
        }

        println!("Fetched {} messages", messages.len() + res.len());

        last_id = res.first().unwrap().id.clone();

        res.reverse();

        messages.append(&mut res);
    }

    Ok(messages)
}

async fn delete_messages(options: &Options, messages: &[Message]) -> Result<(), reqwest::Error> {
    let client = reqwest::Client::new();

    for message in messages {
        loop {
            let start = Instant::now();
            let url = format!("https://discord.com/api/v6/channels/{}/messages/{}", options.id, message.id);
            let result = client.delete(&url)
                .header("Authorization", &options.token)
                .send()
                .await;

            match result {
                Err(_) => continue,
                Ok(res) => {
                    let text = res.text().await?;

                    if text.is_empty() {
                        println!("Deleted {id} in {ms}ms",
                            id = message.id,
                            ms = start.elapsed().as_millis()
                        );
                        task::sleep(Duration::from_millis(1200)).await;
                        break;
                    }

                    let maybe_error: Result<RatelimitError, _> = serde_json::from_str(&text);

                    if let Ok(error) = maybe_error {
                        let delay = error.retry_after;
                        println!("Hit a ratelimit! Waiting {}ms...", delay);

                        task::sleep(Duration::from_millis(delay + 1000)).await;
                    } else {
                        eprintln!("Uncaught error in network response! {}", &text);
                    }

                    continue;
                }
            }
        }
    }

    Ok(())
}

#[async_std::main]
async fn main() -> Result<(), reqwest::Error> {
    let mut options = Options::from_args();

    let start = Instant::now();

    let info = fetch_info(&options).await?;
    println!("Hello, {}!", info.username);
    println!("Fetching messages from {}...", options.id);

    options.target.get_or_insert_with(|| info.id.clone());
    options.start.get_or_insert_with(|| "0".to_owned());

    let messages = fetch_messages(&options).await?;
    println!("Found {} messages", messages.len());

    let mut messages: Vec<Message> = messages.into_iter()
        .filter(|message| message.author.id == *options.target.as_ref().unwrap())
        .filter(|message| [0, 4, 6].contains(&message.r#type))
        // .filter(|message| !message.attachments.is_empty())
        .collect();

    if let Some(ref exclusions) = options.exclude {
        dbg!(exclusions);

        messages = messages.into_iter()
            .filter(|message| !exclusions.contains(&message.id))
            .collect();
    }

    println!("Found {} own messages", messages.len());

    delete_messages(&options, &messages).await?;

    println!("Done! It took {}ms to delete {} messages",
        start.elapsed().as_millis(),
        messages.len()
    );

    Ok(())
}
