use crate::model::CommandTask;
use regex::Regex;
use serde_json::Value;

pub const EXEC_MARKER: &str = "notionSSH-executed";

pub fn parse_command_from_block(block: &Value) -> Option<CommandTask> {
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

pub fn children_contains_marker(children: &[Value]) -> bool {
    for child in children.iter() {
        if child.get("type").and_then(|v| v.as_str()) == Some("code") {
            if let Some(arr) = child
                .get("code")
                .and_then(|c| c.get("rich_text"))
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
                if s.contains(EXEC_MARKER) {
                    return true;
                }
            }
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parse_command_ok() {
        let block = json!({
            "id": "abc",
            "type": "paragraph",
            "created_time": "2025-01-01T00:00:00.000Z",
            "created_by": {"id": "user_123"},
            "paragraph": {"rich_text": [{"plain_text": "!(docker ps)"}]}
        });
        let task = parse_command_from_block(&block).expect("should parse");
        assert_eq!(task.block_id, "abc");
        assert_eq!(task.command, "docker ps");
        assert_eq!(task.created_by_id.as_deref(), Some("user_123"));
    }

    #[test]
    fn parse_command_none() {
        let block = json!({
            "id": "abc",
            "type": "paragraph",
            "paragraph": {"rich_text": [{"plain_text": "echo hello"}]}
        });
        assert!(parse_command_from_block(&block).is_none());
    }

    #[test]
    fn children_marker_detection() {
        let ok = json!({
            "type": "code",
            "code": {"rich_text": [{"text": {"content": "... # notionSSH-executed"}}]}
        });
        assert!(children_contains_marker(&[ok]));

        let miss = json!({
            "type": "code",
            "code": {"rich_text": [{"text": {"content": "no marker"}}]}
        });
        assert!(!children_contains_marker(&[miss]));
    }
}

