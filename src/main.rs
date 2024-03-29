use std::env;
use futures::future::join_all;
use reqwest::{header::USER_AGENT, Error};
use serde::{Serialize, Deserialize};
use tl::Node::Tag;
use tokio::{fs::File, io::{AsyncWriteExt, AsyncReadExt}};
use urlencoding::encode;

const WIKI_URL: &'static str = "https://oldschool.runescape.wiki/";
const WIKI_IMAGES_URL: &'static str = "https://oldschool.runescape.wiki/images/";
const FF_USER_AGENT: &'static str = "Mozilla/5.0 (platform; rv:geckoversion) Gecko/geckotrail Firefox/firefoxversion";

#[derive(Serialize, Deserialize, Debug)]
struct DataEntry {
    name: String,
    gender: String,
    race: String,
    region: String,

    #[serde(rename(serialize = "combatLevel", deserialize = "combatLevel"))]
    combat_level: i32,
    
    #[serde(rename(serialize = "releaseDate", deserialize = "releaseDate"))]
    release_date: i32,

    #[serde(default)]
    image: String,
}

#[derive(Serialize, Deserialize, Debug)]
struct DataList {
    npcs: Vec<DataEntry>
}

fn make_name_urlsafe(name: &str) -> String {
    let url_param = name.replace(" ", "_");

    return String::from(encode(&url_param));
}

async fn request_chathead(entry: &DataEntry) -> Result<Option<String>, Error> {
    let base_url = WIKI_IMAGES_URL;
    let url_param = make_name_urlsafe(&entry.name) + "_chathead.png";
    
    let url = format!("{}{}", base_url, url_param);
    let client = reqwest::Client::new();

    let resp = client.get(&url)
        .header(USER_AGENT, FF_USER_AGENT)
        .send()
        .await?;

    if !resp.status().is_success() { return Ok(None); } 

    return Ok(Some(url));
}

async fn request_infobox_image_source_from_wiki(entry: &DataEntry) -> Result<Option<String>, Error> {
    fn transform_wiki_link_to_img_source(wiki_link: &str) -> String {
        /* Transforms /w/File:Zulrah_(serpentine).png to images/Zulrah_(serpentine).png */
        let base_url = WIKI_IMAGES_URL;

        let url_param = wiki_link.replace("/w/File:", "");
        let url_param = encode(&url_param);
    
        format!("{}{}", base_url, url_param)
    }

    let base_url = WIKI_URL;
    let url_param = make_name_urlsafe(&entry.name);

    let url = format!("{}{}", base_url, url_param);

    let client = reqwest::Client::new();

    let resp = client.get(&url)
        .header(USER_AGENT, FF_USER_AGENT)
        .send()
        .await?;

    let html = resp.text().await?;
    let dom = tl::parse(&html, tl::ParserOptions::default()).unwrap();
    let parser = dom.parser();

    let handle = dom.query_selector(".image").and_then(|mut iter| iter.next()).unwrap();
    let first_image_node = handle.get(parser);

    if first_image_node.is_none() { return Ok(None); }

    if let Tag(html_tag) = first_image_node.unwrap() {
        let url = html_tag.attributes().get("href").unwrap().unwrap().as_utf8_str();

        return Ok(Some(transform_wiki_link_to_img_source(&url)));
        
    } else {
        return Ok(None);
    }
}

async fn set_image_source(entry: &mut DataEntry) {
    println!("Processing: {}", &entry.name);

    if let Ok(Some(image)) = request_chathead(&entry).await {
        entry.image = image;
        return;
    }

    if let Ok(Some(image)) = request_infobox_image_source_from_wiki(&entry).await {
        entry.image = image;
        return;
    }
}

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let args: Vec<String> = env::args().collect();

    let default_input_location = String::from("input.json");
    let default_output_location = String::from("output.json");
    
    let input_location = args.get(1).unwrap_or(&default_input_location);
    let output_location = args.get(2).unwrap_or(&default_output_location);

    let mut contents = String::new();
    let mut input = File::open(input_location).await?;
    input.read_to_string(&mut contents).await?;

    let mut datalist = serde_json::from_str::<DataList>(&contents).unwrap();
    let mut tasks = Vec::new();

    for entry in &mut datalist.npcs {
        tasks.push(set_image_source(entry));
    }

    join_all(tasks).await;

    let mut output = File::create(output_location).await?;
    output.write_all(serde_json::to_string_pretty(&datalist)?.as_bytes()).await?;

    Ok(())
}
