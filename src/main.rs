use structopt::StructOpt;
use serde::{Serialize, Deserialize};
use reqwest;
use std::time::{Duration, Instant};
use async_std::task;
use serde_json::{Value, json};

#[derive(Debug, StructOpt)]
#[structopt(name = "eater", about = "Eats Discord DMs, whole.")]
struct Options {
    #[structopt(long)]
    id: String,

    #[structopt(long)]
    token: String,
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

    return Ok(res);
}

#[derive(Serialize, Deserialize)]
struct Message {
    r#type: i32,
    id: String,
    author: Author
}

#[derive(Serialize, Deserialize)]
struct Author {
    id: String
}

#[derive(Serialize, Deserialize)]
struct RatelimitError {
    message: String,
    retry_after: i32
}

async fn fetch_messages(options: &Options) -> Result<Vec<Message>, reqwest::Error> {
    let client = reqwest::Client::new();
    let mut last_id = "0".to_string();
    let mut messages = vec![];

    loop {
        let url = format!("https://discord.com/api/v6/channels/{}/messages?after={}&limit=100", &options.id, last_id);
        let mut res = client.get(&url)
            .header("Authorization", &options.token)
            .send()
            .await?
            .json::<Vec<Message>>()
            .await?;

        if res.len() == 0 {
            break
        }

        last_id = res.first().unwrap().id.clone();

        res.reverse();

        messages.append(&mut res);
    }

    Ok(messages)
}

async fn delete_messages(options: &Options, messages: &Vec<Message>) -> Result<(), reqwest::Error> {
    let client = reqwest::Client::new();

    for message in messages {
        loop {
            let start = Instant::now();
            let url = format!("https://discord.com/api/v6/channels/{}/messages/{}", options.id, message.id);
            let res = client.delete(&url)
                .header("Authorization", &options.token)
                .send()
                .await;

            match res {
                Err(_) => continue,
                Ok(test) => {
                    let text = test.text().await?;

                    if text == "" {
                        println!("Deleted {} in {}ms", message.id, start.elapsed().as_millis());
                        break;
                    } else {
                        let v: Value = serde_json::from_str(&text)
                            .or::<Value>(Ok(json!({})))
                            .unwrap();

                        let delay = v.get("retry_after")
                            .or(Some(&json!(5000)))
                            .unwrap()
                            .as_u64()
                            .expect("retry_after is an integer");

                        println!("Hit a ratelimit! Waiting {}ms...", delay);
                        task::sleep(Duration::from_millis(delay)).await;
                        continue;
                    }
                }
            }
        }
    }

    Ok(())
}

#[async_std::main]
async fn main() -> Result<(), reqwest::Error> {
    let options = Options::from_args();

    let info = fetch_info(&options).await?;
    println!("Hello, {}!", info.username);
    println!("Fetching messages from {}...", options.id);

    let messages = fetch_messages(&options).await?;
    println!("Found {} messages", messages.len());

    let messages: Vec<Message> = messages.into_iter()
        .filter(|message| message.author.id == info.id && message.r#type == 0)
        .collect();

    println!("Found {} own messages", messages.len());

    delete_messages(&options, &messages).await?;

    Ok(())
}
