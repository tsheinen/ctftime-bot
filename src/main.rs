use chrono::{DateTime, Datelike, Duration, NaiveDate, NaiveDateTime, NaiveTime, Utc, Weekday};
use futures::FutureExt;
use lambda::{handler_fn, Context};
use select::document::Document;
use select::predicate::{Class, Descendant, Element};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::HashMap;
use std::ops::Add;

#[derive(Serialize, Deserialize, Debug, Clone)]
struct Event {
    organizers: Vec<Organizer>,
    onsite: bool,
    finish: String,
    description: String,
    weight: f64,
    title: String,
    url: String,
    is_votable_now: bool,
    restrictions: String,
    format: String,
    start: String,
    ctftime_url: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct Organizer {
    name: String,
    icon_url: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct Embed {
    title: String,
    description: String,
    url: String,
    color: u64,
    author: Organizer,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct Image {
    src: String,
    height: i32,
    width: i32,
}

async fn lambda_handler(_: serde_json::Value, _: Context) -> Result<(), Error> {
    let args = std::env::var("DISCORD_WEBHOOKS")
        .unwrap()
        .split(',')
        .filter_map(|x| {
            let mut it = x.split(':');
            Some((it.next()?.parse::<u64>().ok()?, it.next()?.to_string()))
        })
        .collect::<Vec<_>>();

    let (start, end) = {
        let now = chrono::offset::Local::now();
        let current_year = now.year();
        let current_week = now.iso_week().week();
        let next_week = now.add(Duration::weeks(1)).iso_week().week();
        (
            DateTime::<Utc>::from_utc(
                NaiveDateTime::new(
                    NaiveDate::from_isoywd(current_year, current_week, Weekday::Fri),
                    NaiveTime::from_num_seconds_from_midnight(0, 0),
                ),
                Utc,
            )
            .timestamp(),
            DateTime::<Utc>::from_utc(
                NaiveDateTime::new(
                    NaiveDate::from_isoywd(current_year, next_week, Weekday::Mon),
                    NaiveTime::from_num_seconds_from_midnight(0, 0),
                ),
                Utc,
            )
            .timestamp(),
        )
    };

    let events = reqwest::get(&format!(
        "https://ctftime.org/api/v1/events/?limit=100&start={start}&finish={end}"
    ))
    .await?
    .json::<Vec<Event>>()
    .await?;

    let icons = get_icons(events.iter().map(|x| x.ctftime_url.as_str())).await;

    let embeds_all = events
        .into_iter()
        .map(|mut ev| Embed {
            title: ev.title,
            description: {
                if ev.description.len() > 100 {
                    ev.description[0..100].to_string() + "..."
                } else {
                    ev.description
                }
            },
            url: ev.url,
            color: 7506394,
            author: {
                let mut org = ev.organizers.swap_remove(0);
                org.icon_url = icons.get(&ev.ctftime_url).cloned().map(|x| x.src);
                org
            },
        })
        .collect::<Vec<_>>();

    let client = reqwest::Client::new();
    for embeds in embeds_all.chunks(10) {
        for (id, token) in &args {
            client
                .post(&format!(
                    "https://discord.com/api/webhooks/{}/{}",
                    id, token
                ))
                .json(&json!({ "embeds": embeds }))
                .send()
                .await?
                .text()
                .await?;
        }
    }
    Ok(())
}

type Error = Box<dyn std::error::Error + Send + Sync + 'static>;

async fn get_icons<'a>(items: impl IntoIterator<Item = &'a str>) -> HashMap<String, Image> {
    let futs = futures::future::join_all(
        items
            .into_iter()
            .map(|ctftime_url| (ctftime_url.to_string(), get_icon(ctftime_url)))
            .map(|(name, fut)| fut.map(|x| (name, x))),
    );
    futs
        .await
        .into_iter()
        .filter_map(|(name, x)| Some((name, x?)))
        .collect::<HashMap<String, Image>>()
}

async fn get_icon(url: &str) -> Option<Image> {
    let document: Document = reqwest::get(url)
        .await
        .ok()?
        .text()
        .await
        .ok()?
        .as_str()
        .into();
    let node = document.find(Descendant(Class("span2"), Element)).next()?;
    let src = node.attr("src").unwrap_or("static/images/nologo.png");
    let width = node.attr("width").and_then(|x| x.parse().ok()).unwrap_or(0);
    let height = node.attr("height").and_then(|x| x.parse().ok()).unwrap_or(0);
    Some(Image {
        src: format!("https://ctftime.org/{src}"),
        width,
        height,
    })
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    let func = handler_fn(lambda_handler);
    lambda::run(func).await?;
    Ok(())
}
