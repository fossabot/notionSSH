use anyhow::Result;
use std::env;
use std::io::{self, Write};
use regex::Regex;

#[derive(Debug, Clone)]
pub struct Config {
    pub api_key: String,
    pub page_url: String,
}

pub fn load_config() -> Result<Config> {
    let api_key = env::var("NotionAPIKey").or_else(|_| env::var("NOTION_API_KEY")).unwrap_or_else(|_| {
        prompt("NOTION_API_KEY : ")
    });

    let mut page_url = env::var("NotionPageURL").or_else(|_| env::var("NOTION_PAGE_URL")).unwrap_or_else(|_| {
        prompt("NOTION_PAGE_URL : ")
    });

    while !is_valid_http_https_url(&page_url) {
        eprintln!("Invalid URL. Please enter http/https URL with valid domain.");
        page_url = prompt("NOTION_PAGE_URL : ");
    }

    Ok(Config { api_key, page_url })
}

fn prompt(label: &str) -> String {
    print!("{}", label);
    let _ = io::stdout().flush();
    let mut s = String::new();
    io::stdin().read_line(&mut s).expect("failed to read input");
    s.trim().to_string()
}

pub fn is_valid_http_https_url(url: &str) -> bool {
    let re = Regex::new(r"^(?i)https?://([a-z0-9-]+\.)+[a-z]{2,}(:\d+)?(/.*)?$").unwrap();
    re.is_match(url.trim())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn url_validation() {
        assert!(is_valid_http_https_url("http://example.com"));
        assert!(is_valid_http_https_url("https://sub.domain.co.kr/path?q=1"));
        assert!(is_valid_http_https_url("https://notion.so"));
        assert!(is_valid_http_https_url("https://www.notion.so/page-123"));
        assert!(is_valid_http_https_url("https://example.com:443/path"));

        assert!(!is_valid_http_https_url("ftp://example.com"));
        assert!(!is_valid_http_https_url("http://localhost"));
        assert!(!is_valid_http_https_url("https://invalid_domain"));
        assert!(!is_valid_http_https_url("not a url"));
    }
}

