use crate::model::PaginatedBlocks;
use crate::parser::{children_contains_marker, EXEC_MARKER};
use crate::util::node_name;
use anyhow::{anyhow, Result};
use reqwest::blocking::Client;
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE};
use serde_json::{json, Value};

pub const NOTION_VERSION: &str = "2022-06-28";
pub const STATUS_MARKER: &str = "notionSSH-status";

pub fn build_client(api_key: &str) -> Result<Client> {
    let mut headers = reqwest::header::HeaderMap::new();
    headers.insert(AUTHORIZATION, format!("Bearer {}", api_key).parse().unwrap());
    headers.insert("Notion-Version", NOTION_VERSION.parse().unwrap());
    let client = Client::builder().default_headers(headers).build()?;
    Ok(client)
}

pub fn fetch_all_children(client: &Client, block_id: &str) -> Result<Vec<Value>> {
    let mut results = Vec::new();
    let mut cursor: Option<String> = None;
    loop {
        let mut url = format!(
            "https://api.notion.com/v1/blocks/{}/children?page_size=100",
            block_id
        );
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

pub fn is_block_processed(client: &Client, block_id: &str) -> Result<bool> {
    let url = format!(
        "https://api.notion.com/v1/blocks/{}/children?page_size=50",
        block_id
    );
    let resp = client.get(&url).send()?;
    if !resp.status().is_success() {
        return Ok(false);
    }
    let body: PaginatedBlocks = resp.json()?;
    Ok(children_contains_marker(&body.results))
}

pub fn lookup_user_email(client: &Client, user_id: &str) -> Option<String> {
    if user_id.is_empty() {
        return None;
    }
    let url = format!("https://api.notion.com/v1/users/{}", user_id);
    let resp = client.get(&url).send().ok()?;
    if !resp.status().is_success() {
        return None;
    }
    let v: Value = resp.json().ok()?;
    v.get("person")
        .and_then(|p| p.get("email"))
        .and_then(|e| e.as_str())
        .map(|s| s.to_string())
}

pub fn build_result_payload(cmd: &str, output: &str, email: &str) -> Value {
    use chrono::{Local, SecondsFormat};
    let now = Local::now().to_rfc3339_opts(SecondsFormat::Secs, true);
    let machine = node_name();
    let mut body_text = String::new();
    body_text.push_str("$ ");
    body_text.push_str(cmd);
    body_text.push_str("\n");
    body_text.push_str(output);
    body_text.push_str("\n---\n");
    body_text.push_str(&format!(
        "executed_by={} | node={} | {}\n# {}",
        email, machine, now, EXEC_MARKER
    ));

    json!({
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
    })
}

pub fn append_result_children(
    client: &Client,
    block_id: &str,
    cmd: &str,
    output: &str,
    email: &str,
) -> Result<()> {
    let payload = build_result_payload(cmd, output, email);
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

pub fn find_status_block(client: &Client, page_id: &str) -> Result<Option<String>> {
    let children = fetch_all_children(client, page_id)?;
    for child in children.iter() {
        if child.get("type").and_then(|v| v.as_str()) == Some("paragraph") {
            if let Some(arr) = child
                .get("paragraph")
                .and_then(|p| p.get("rich_text"))
                .and_then(|v| v.as_array())
            {
                let mut s = String::new();
                for r in arr {
                    if let Some(t) = r.get("plain_text").and_then(|v| v.as_str()) {
                        s.push_str(t);
                    } else if let Some(t) = r
                        .get("text")
                        .and_then(|t| t.get("content"))
                        .and_then(|v| v.as_str())
                    {
                        s.push_str(t);
                    }
                }
                if s.contains(STATUS_MARKER) {
                    if let Some(id) = child.get("id").and_then(|v| v.as_str()) {
                        return Ok(Some(id.to_string()));
                    }
                }
            }
        }
    }
    Ok(None)
}

pub fn ensure_status_block(client: &Client, page_id: &str) -> Result<String> {
    if let Some(id) = find_status_block(client, page_id)? { return Ok(id); }
    let text = "[*] NotionSSH is Loading - waiting for commands. Press Ctrl+C to stop. # notionSSH-status";
    let payload = json!({
        "children": [
            {
                "object": "block",
                "type": "paragraph",
                "paragraph": {
                    "rich_text": [{"type": "text", "text": {"content": text}}]
                }
            }
        ]
    });
    let url = format!("https://api.notion.com/v1/blocks/{}/children", page_id);
    let resp = client
        .patch(&url)
        .header(CONTENT_TYPE, "application/json")
        .body(payload.to_string())
        .send()?;
    if !resp.status().is_success() {
        let status = resp.status();
        let txt = resp.text().unwrap_or_default();
        return Err(anyhow!("Failed to create status block: {} - {}", status, txt));
    }
    // Fetch again to get the new block id
    let new_id = find_status_block(client, page_id)?.ok_or_else(|| anyhow!("Status block not found after creation"))?;
    Ok(new_id)
}

pub fn update_status_block(client: &Client, status_block_id: &str, text: &str) -> Result<()> {
    let payload = json!({
        "paragraph": {
            "rich_text": [{"type": "text", "text": {"content": text}}]
        }
    });
    let url = format!("https://api.notion.com/v1/blocks/{}", status_block_id);
    let resp = client
        .patch(&url)
        .header(CONTENT_TYPE, "application/json")
        .body(payload.to_string())
        .send()?;
    if !resp.status().is_success() {
        let status = resp.status();
        let txt = resp.text().unwrap_or_default();
        return Err(anyhow!("Failed to update status block: {} - {}", status, txt));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn payload_contains_marker_and_metadata() {
        let v = build_result_payload("echo hi", "hello", "u@example.com");
        let children = v.get("children").unwrap().as_array().unwrap();
        assert_eq!(children.len(), 2);
        let code = &children[0];
        let rich = code
            .get("code")
            .unwrap()
            .get("rich_text")
            .unwrap()
            .as_array()
            .unwrap();
        let content = rich[0]
            .get("text")
            .unwrap()
            .get("content")
            .unwrap()
            .as_str()
            .unwrap()
            .to_string();
        assert!(content.contains("notionSSH-executed"));
        assert!(content.contains("executed_by=u@example.com"));

        let para = &children[1];
        let meta = para
            .get("paragraph")
            .unwrap()
            .get("rich_text")
            .unwrap()
            .as_array()
            .unwrap();
        let mcontent = meta[0]
            .get("text")
            .unwrap()
            .get("content")
            .unwrap()
            .as_str()
            .unwrap();
        assert!(mcontent.contains("email: u@example.com"));
        assert!(mcontent.contains("machine:"));
    }
}
