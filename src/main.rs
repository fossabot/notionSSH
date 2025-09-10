use anyhow::{anyhow, Context, Result};
use chrono::{Datelike, Local, SecondsFormat};
use regex::Regex;
use reqwest::blocking::Client;
use reqwest::blocking::Response;
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE};
use serde::Deserialize;
use serde_json::{json, Value};
use std::env;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::Path;
use std::process::Command;
use url::Url;

const NOTION_VERSION: &str = "2022-06-28";
const EXEC_MARKER: &str = "notionSSH-executed";

#[derive(Debug, Clone)]
struct Config {
    api_key: String,
    page_url: String,
}

#[derive(Debug, Clone)]
struct CommandTask {
    block_id: String,
    command: String,
    created_time: String,
    created_by_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct PaginatedBlocks {
    results: Vec<Value>,
    next_cursor: Option<String>,
    has_more: bool,
}

fn main() {
    if let Err(e) = run() {
        eprintln!("error: {e:#}");
        // Best-effort audit log on fatal failure
        let _ = write_audit_log("<startup>", "<unknown>", &Local::now().to_rfc3339(), &node_name(), "failed");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let cfg = load_config()?;
    let client = build_client(&cfg.api_key)?;
    let page_id = extract_page_id(&cfg.page_url)?;

    let blocks = fetch_all_children(&client, &page_id)?;
    let mut tasks = Vec::new();
    for b in blocks.iter() {
        if let Some(t) = parse_command_from_block(b) {
            // Skip if already processed
            if is_block_processed(&client, &t.block_id)? {
                continue;
            }
            tasks.push(t);
        }
    }

    if tasks.is_empty() {
        println!("No new commands found.");
        return Ok(());
    }

    for task in tasks {
        let requester_id = task.created_by_id.clone().unwrap_or_else(|| "unknown".to_string());
        let requester_email = lookup_user_email(&client, task.created_by_id.as_deref().unwrap_or(""))
            .unwrap_or_else(|| "unknown".to_string());

        let (out, status) = execute_command(&task.command)?;

        // Write logs
        write_command_log(&requester_id, &task.command, &task.created_time)?;
        write_audit_log(&task.command, &requester_email, &task.created_time, &node_name(), if status { "success" } else { "failed" })?;

        // Append results under the command block as children
        append_result_children(&client, &task.block_id, &task.command, &out, &requester_email)?;
    }

    Ok(())
}

fn load_config() -> Result<Config> {
    let api_key = env::var("NotionAPIKey")
        .or_else(|_| env::var("NOTION_API_KEY"))
        .context("Missing NotionAPIKey/NOTION_API_KEY env var")?;
    let page_url = env::var("NotionPageURL")
        .or_else(|_| env::var("NOTION_PAGE_URL"))
        .context("Missing NotionPageURL/NOTION_PAGE_URL env var")?;
    Ok(Config { api_key, page_url })
}

fn build_client(api_key: &str) -> Result<Client> {
    let mut headers = reqwest::header::HeaderMap::new();
    headers.insert(
        AUTHORIZATION,
        format!("Bearer {}", api_key).parse().unwrap(),
    );
    headers.insert("Notion-Version", NOTION_VERSION.parse().unwrap());
    let client = Client::builder().default_headers(headers).build()?;
    Ok(client)
}

fn extract_page_id(page_url: &str) -> Result<String> {
    // Accept raw ID or full URL. Extract the 32-hex id and hyphenate it.
    let mut s = page_url.trim().to_string();
    if let Ok(url) = Url::parse(page_url) {
        if let Some(seg) = url.path_segments().and_then(|mut it| it.last()) {
            s = seg.to_string();
        }
    }
    // find the last 32 hex chars ignoring hyphens
    let re = Regex::new(r"([0-9a-fA-F-]{32,})$").unwrap();
    let caps = re
        .captures(&s)
        .or_else(|| {
            // Fallback: strip everything non-hex
            let cleaned: String = s.chars().filter(|c| c.is_ascii_hexdigit()).collect();
            if cleaned.len() >= 32 {
                Some(Regex::new(r"(.*)").unwrap().captures(&cleaned))
                    .flatten()
            } else {
                None
            }
        })
        .ok_or_else(|| anyhow!("Cannot extract Notion page id from URL: {page_url}"))?;

    let raw = caps.get(1).unwrap().as_str().replace('-', "");
    if raw.len() < 32 {
        return Err(anyhow!("Invalid page id in URL"));
    }
    let id32 = &raw[raw.len() - 32..];
    Ok(hyphenate_id(id32))
}

fn hyphenate_id(id32: &str) -> String {
    format!(
        "{}-{}-{}-{}-{}",
        &id32[0..8],
        &id32[8..12],
        &id32[12..16],
        &id32[16..20],
        &id32[20..32]
    )
}

fn fetch_all_children(client: &Client, block_id: &str) -> Result<Vec<Value>> {
    let mut results = Vec::new();
    let mut cursor: Option<String> = None;
    loop {
        let mut url = format!("https://api.notion.com/v1/blocks/{}/children?page_size=100", block_id);
        if let Some(c) = &cursor {
            url.push_str(&format!("&start_cursor={}", c));
        }
        let resp = client.get(&url).send()?;
        let status = resp.status();
        if !status.is_success() {
            return Err(anyhow!("Notion API error fetching children: {}", status));
        }
        let body: PaginatedBlocks = resp.json()?;
        for r in body.results {
            results.push(r);
        }
        if body.has_more {
            cursor = body.next_cursor;
        } else {
            break;
        }
    }
    Ok(results)
}

fn parse_command_from_block(block: &Value) -> Option<CommandTask> {
    let Some(obj_type) = block.get("type").and_then(|v| v.as_str()) else { return None };
    if obj_type != "paragraph" && obj_type != "to_do" { return None; }

    let rich = block.get(obj_type)?.get("rich_text")?.as_array()?;
    let mut text = String::new();
    for r in rich {
        if let Some(t) = r.get("plain_text").and_then(|v| v.as_str()) {
            text.push_str(t);
        } else if let Some(t) = r
            .get("text")
            .and_then(|t| t.get("content"))
            .and_then(|v| v.as_str())
        {
            text.push_str(t);
        }
    }
    let text = text.trim();
    let re = Regex::new(r"^!\((?P<cmd>.+)\)$").unwrap();
    let caps = re.captures(text)?;
    let cmd = caps.name("cmd")?.as_str().trim().to_string();
    let block_id = block.get("id")?.as_str()?.to_string();
    let created_time = block
        .get("created_time")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let created_by_id = block
        .get("created_by")
        .and_then(|v| v.get("id"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    Some(CommandTask { block_id, command: cmd, created_time, created_by_id })
}

fn is_block_processed(client: &Client, block_id: &str) -> Result<bool> {
    // Fetch children and look for a code block with our marker
    let url = format!("https://api.notion.com/v1/blocks/{}/children?page_size=50", block_id);
    let resp = client.get(&url).send()?;
    if !resp.status().is_success() {
        return Ok(false);
    }
    let body: PaginatedBlocks = resp.json()?;
    for child in body.results.iter() {
        if child.get("type").and_then(|v| v.as_str()) == Some("code") {
            if let Some(arr) = child.get("code").and_then(|c| c.get("rich_text")).and_then(|v| v.as_array()) {
                let mut s = String::new();
                for r in arr {
                    if let Some(t) = r.get("plain_text").and_then(|v| v.as_str()) { s.push_str(t); }
                    else if let Some(t) = r.get("text").and_then(|t| t.get("content")).and_then(|v| v.as_str()) { s.push_str(t); }
                }
                if s.contains(EXEC_MARKER) { return Ok(true); }
            }
        }
    }
    Ok(false)
}

fn lookup_user_email(client: &Client, user_id: &str) -> Option<String> {
    if user_id.is_empty() { return None; }
    let url = format!("https://api.notion.com/v1/users/{}", user_id);
    let resp = client.get(&url).send().ok()?;
    if !resp.status().is_success() { return None; }
    let v: Value = resp.json().ok()?;
    v.get("person")
        .and_then(|p| p.get("email"))
        .and_then(|e| e.as_str())
        .map(|s| s.to_string())
}

fn execute_command(cmd: &str) -> Result<(String, bool)> {
    #[cfg(target_os = "windows")]
    let output = Command::new("cmd").args(["/C", cmd]).output().with_context(|| format!("Failed to run command: {}", cmd))?;
    #[cfg(not(target_os = "windows"))]
    let output = Command::new("sh").arg("-lc").arg(cmd).output().with_context(|| format!("Failed to run command: {}", cmd))?;

    let mut combined = String::new();
    if !output.stdout.is_empty() {
        combined.push_str(&String::from_utf8_lossy(&output.stdout));
    }
    if !output.stderr.is_empty() {
        if !combined.is_empty() { combined.push_str("\n"); }
        combined.push_str(&String::from_utf8_lossy(&output.stderr));
    }
    // Limit output length to keep Notion happy
    let max = 16_000; // chars
    if combined.len() > max {
        combined = combined[..max].to_string();
        combined.push_str("\n... [truncated]\n");
    }
    Ok((combined, output.status.success()))
}

fn node_name() -> String {
    // Try common env vars first for speed
    if let Ok(s) = env::var("COMPUTERNAME") { return s; }
    if let Ok(s) = env::var("HOSTNAME") { return s; }
    // Fallback: hostname command
    let name = hostname::get().ok().and_then(|os| os.into_string().ok()).unwrap_or_else(|| "unknown".to_string());
    name
}

fn append_result_children(client: &Client, block_id: &str, cmd: &str, output: &str, email: &str) -> Result<()> {
    let now = Local::now().to_rfc3339_opts(SecondsFormat::Secs, true);
    let machine = node_name();
    let mut body_text = String::new();
    body_text.push_str("$ ");
    body_text.push_str(cmd);
    body_text.push_str("\n");
    body_text.push_str(output);
    body_text.push_str("\n---\n");
    body_text.push_str(&format!("executed_by={} | node={} | {}\n# {}", email, machine, now, EXEC_MARKER));

    let payload = json!({
        "children": [
            {
                "object": "block",
                "type": "code",
                "code": {
                    "rich_text": [{
                        "type": "text",
                        "text": {"content": body_text}
                    }],
                    "language": "plain text"
                }
            },
            {
                "object": "block",
                "type": "paragraph",
                "paragraph": {
                    "rich_text": [{
                        "type": "text",
                        "text": {"content": format!("email: {} | machine: {}", email, machine)}
                    }]
                }
            }
        ]
    });

    let url = format!("https://api.notion.com/v1/blocks/{}/children", block_id);
    let resp = client
        .patch(&url)
        .header(CONTENT_TYPE, "application/json")
        .body(payload.to_string())
        .send()?;
    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().unwrap_or_default();
        return Err(anyhow!("Failed to append Notion children: {} - {}", status, text));
    }
    Ok(())
}

fn write_command_log(user_id: &str, command: &str, req_time_iso: &str) -> Result<()> {
    // Ensure ./logs directory
    let logs_dir = Path::new("./logs");
    if !logs_dir.exists() { fs::create_dir_all(logs_dir)?; }
    let today = Local::now();
    let filename = format!("command.{:04}{:02}{:02}.log", today.year(), today.month(), today.day());
    let path = logs_dir.join(filename);
    let mut f = OpenOptions::new().create(true).append(true).open(&path)?;
    let line = format!("[{}] ({}) : {}\n", req_time_iso, user_id, command);
    f.write_all(line.as_bytes())?;
    Ok(())
}

fn write_audit_log(command: &str, requester: &str, req_time_iso: &str, node: &str, status: &str) -> Result<()> {
    let mut f = OpenOptions::new()
        .create(true)
        .append(true)
        .open("./log")?;
    // CSV-like: command, requester, request_time, node, status
    let line = format!("{} , {} , {} , {} , {}\n", command, requester, req_time_iso, node, status);
    f.write_all(line.as_bytes())?;
    Ok(())
}

// Minimal dependency: tiny hostname helper using std. Provide module to avoid extra crate if unavailable.
mod hostname {
    use std::ffi::OsString;
    pub fn get() -> std::io::Result<OsString> {
        #[cfg(target_os = "windows")]
        {
            use std::env;
            if let Ok(s) = env::var("COMPUTERNAME") { return Ok(OsString::from(s)); }
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
