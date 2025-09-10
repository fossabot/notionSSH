use anyhow::{Context, Result};
use std::env;

#[derive(Debug, Clone)]
pub struct Config {
    pub api_key: String,
    pub page_url: String,
}

pub fn load_config() -> Result<Config> {
    let api_key = env::var("NotionAPIKey")
        .or_else(|_| env::var("NOTION_API_KEY"))
        .context("Missing NotionAPIKey/NOTION_API_KEY env var")?;
    let page_url = env::var("NotionPageURL")
        .or_else(|_| env::var("NOTION_PAGE_URL"))
        .context("Missing NotionPageURL/NOTION_PAGE_URL env var")?;
    Ok(Config { api_key, page_url })
}

