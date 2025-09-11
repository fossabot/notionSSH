use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::HashMap;
use std::fs;
use std::io::{Read, Write};
use std::path::Path;

const ACCESS_PATH: &str = ".notionSSH/access.json";

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PermRule {
    pub allow: Vec<String>,
    pub deny: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccessFile {
    /// Map of email -> role name (e.g., "perm_user", "default")
    #[serde(default)]
    pub emails: HashMap<String, String>,
    /// List of manager emails who are always allowed
    #[serde(default)]
    pub perm_manager: Vec<String>,
    /// Permissions by role name. Must include "default"
    pub perms: HashMap<String, PermRule>,
}

impl Default for AccessFile {
    fn default() -> Self {
        let mut perms = HashMap::new();
        perms.insert(
            "default".to_string(),
            PermRule {
                allow: vec!["*".to_string()],
                deny: vec![],
            },
        );
        Self { emails: HashMap::new(), perm_manager: Vec::new(), perms }
    }
}

pub fn load_or_create() -> Result<AccessFile> {
    let path = Path::new(ACCESS_PATH);
    if !path.exists() {
        if let Some(parent) = path.parent() {
            if !parent.exists() {
                fs::create_dir_all(parent)?;
            }
        }
        let default = AccessFile::default();
        let s = serde_json::to_string_pretty(&default)?;
        let mut f = fs::File::create(path)?;
        f.write_all(s.as_bytes())?;
        println!("[!] access.json has been created, but by default all permissions are allowed. Please update it to match your user settings.");
        return Ok(default);
    }

    let mut s = String::new();
    fs::File::open(path)?.read_to_string(&mut s)?;
    if s.trim().is_empty() {
        return Err(anyhow!("access.json is empty"));
    }
    let af: AccessFile = serde_json::from_str(&s)?;
    // Ensure default exists
    if !af.perms.contains_key("default") {
        return Err(anyhow!("access.json missing required 'perms.default' rule"));
    }
    Ok(af)
}

fn first_token(cmd: &str) -> &str {
    cmd.split_whitespace().next().unwrap_or("")
}

fn matches_rule_item(item: &str, cmd: &str) -> bool {
    if item == "*" { return true; }
    // If item has whitespace, treat as prefix of entire command
    if item.contains(char::is_whitespace) {
        return cmd.starts_with(item);
    }
    // Otherwise compare first token
    let tok = first_token(cmd);
    tok.eq_ignore_ascii_case(item)
}

pub fn is_allowed(af: &AccessFile, email: &str, cmd: &str) -> bool {
    // Managers: always allowed
    if af.perm_manager.iter().any(|e| e.eq_ignore_ascii_case(email)) {
        return true;
    }

    // Resolve role -> rules
    let role = af
        .emails
        .get(email)
        .map(|s| s.as_str())
        .unwrap_or("default");
    let rules = af.perms.get(role).or_else(|| af.perms.get("default"));
    let Some(rules) = rules else { return false };

    // Deny has priority
    if rules.deny.iter().any(|d| matches_rule_item(d, cmd)) {
        return false;
    }

    // Allow
    if rules.allow.iter().any(|a| matches_rule_item(a, cmd)) {
        return true;
    }

    // If no explicit allow matched, deny
    false
}

