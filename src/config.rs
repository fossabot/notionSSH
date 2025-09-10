use anyhow::Result;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::env;
use std::fs;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Config {
    pub api_key: String,
    pub page_url: String,
}

pub fn load_config() -> Result<Config> {
    // 1) Prefer both from env vars
    let env_api = env::var("NotionAPIKey").or_else(|_| env::var("NOTION_API_KEY")).ok();
    let env_url = env::var("NotionPageURL").or_else(|_| env::var("NOTION_PAGE_URL")).ok();
    if let (Some(api_key), Some(page_url)) = (env_api.clone(), env_url.clone()) {
        return Ok(Config { api_key, page_url });
    }

    // 2) Otherwise try storage file
    if let Some(cfg) = load_from_storage_default().ok().flatten() {
        return Ok(cfg);
    }

    // 3) Prompt for both and save
    let api_key = env_api.unwrap_or_else(|| prompt("NOTION_API_KEY : "));
    let mut page_url = env_url.unwrap_or_else(|| prompt("NOTION_PAGE_URL : "));
    while !is_valid_http_https_url(&page_url) {
        eprintln!("Invalid URL. Please enter http/https URL with valid domain.");
        page_url = prompt("NOTION_PAGE_URL : ");
    }
    let cfg = Config { api_key, page_url };
    let _ = save_to_storage_default(&cfg);
    Ok(cfg)
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
    use tempfile::tempdir;

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

    #[test]
    fn storage_roundtrip() {
        let dir = tempdir().unwrap();
        let path = dir.path().join(".notionSSH").join("storage.json");
        let cfg = Config { api_key: "k1".into(), page_url: "https://example.com/p".into() };
        save_to_storage_path(&path, &cfg).unwrap();
        let loaded = load_from_storage_path(&path).unwrap().unwrap();
        assert_eq!(cfg, loaded);
    }
}

fn storage_default_path() -> PathBuf {
    Path::new(".notionSSH").join("storage.json")
}

fn ensure_parent_dir(path: &Path) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        if !parent.exists() {
            fs::create_dir_all(parent)?;
        }
    }
    Ok(())
}

fn load_from_storage_default() -> Result<Option<Config>> {
    load_from_storage_path(&storage_default_path())
}

fn save_to_storage_default(cfg: &Config) -> Result<()> {
    save_to_storage_path(&storage_default_path(), cfg)
}

fn load_from_storage_path(path: &Path) -> Result<Option<Config>> {
    if !path.exists() { return Ok(None); }
    let mut f = fs::File::open(path)?;
    let mut s = String::new();
    f.read_to_string(&mut s)?;
    if s.trim().is_empty() { return Ok(None); }
    let cfg: Config = serde_json::from_str(&s)?;
    Ok(Some(cfg))
}

fn save_to_storage_path(path: &Path, cfg: &Config) -> Result<()> {
    ensure_parent_dir(path)?;
    let s = serde_json::to_string_pretty(cfg)?;
    let mut f = fs::File::create(path)?;
    f.write_all(s.as_bytes())?;
    Ok(())
}
