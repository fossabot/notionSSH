use anyhow::{anyhow, Result};
use regex::Regex;
use std::env;
use url::Url;

pub fn extract_page_id(page_url: &str) -> Result<String> {
    let mut s = page_url.trim().to_string();
    if let Ok(url) = Url::parse(page_url) {
        if let Some(seg) = url.path_segments().and_then(|it| it.last()) {
            s = seg.to_string();
        }
    }
    let re = Regex::new(r"([0-9a-fA-F-]{32,})$").unwrap();
    let raw = if let Some(caps) = re.captures(&s) {
        caps.get(1).unwrap().as_str().replace('-', "")
    } else {
        let cleaned: String = s.chars().filter(|c| c.is_ascii_hexdigit()).collect();
        cleaned
    };

    if raw.len() < 32 {
        return Err(anyhow!("Cannot extract Notion page id from URL: {page_url}"));
    }
    let id32 = &raw[raw.len() - 32..];
    Ok(hyphenate_id(id32))
}

pub fn hyphenate_id(id32: &str) -> String {
    format!(
        "{}-{}-{}-{}-{}",
        &id32[0..8],
        &id32[8..12],
        &id32[12..16],
        &id32[16..20],
        &id32[20..32]
    )
}

pub fn node_name() -> String {
    if let Ok(s) = env::var("COMPUTERNAME") {
        return s;
    }
    if let Ok(s) = env::var("HOSTNAME") {
        return s;
    }
    let name = hostname::get()
        .ok()
        .and_then(|os| os.into_string().ok())
        .unwrap_or_else(|| "unknown".to_string());
    name
}

pub fn os_name() -> String {
    // Basic OS kind from Rust target identifiers
    match std::env::consts::OS {
        "windows" => "Windows".to_string(),
        "linux" => "Linux".to_string(),
        "macos" => "macOS".to_string(),
        other => other.to_string(),
    }
}

// Minimal hostname helper without extra crate.
pub mod hostname {
    use std::ffi::OsString;
    pub fn get() -> std::io::Result<OsString> {
        #[cfg(target_os = "windows")]
        {
            use std::env;
            if let Ok(s) = env::var("COMPUTERNAME") {
                return Ok(OsString::from(s));
            }
        }
        #[cfg(not(target_os = "windows"))]
        {
            use std::process::Command;
            if let Ok(out) = Command::new("hostname").output() {
                let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
                return Ok(OsString::from(s));
            }
        }
        Ok(OsString::from("unknown"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_from_url_and_raw() {
        let id = "0123456789abcdef0123456789abcdef";
        let url = format!("https://www.notion.so/test-{}", id);
        let hy = extract_page_id(&url).unwrap();
        assert!(hy.contains("-"));
        let hy2 = extract_page_id(id).unwrap();
        assert_eq!(hy, hy2);
    }

    #[test]
    fn hyphenate() {
        let id = "0123456789abcdef0123456789abcdef";
        let hy = hyphenate_id(id);
        assert_eq!(hy, "01234567-89ab-cdef-0123-456789abcdef");
    }
}
