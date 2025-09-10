use anyhow::Result;
use chrono::{Datelike, Local, Timelike};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::Path;

pub use crate::util::node_name;

pub fn write_command_log(email: &str, command: &str) -> Result<()> {
    let logs_dir = Path::new("./logs");
    write_command_log_with_dir(logs_dir, email, command)
}

pub fn write_command_log_with_dir(dir: &Path, email: &str, command: &str) -> Result<()> {
    if !dir.exists() {
        fs::create_dir_all(dir)?;
    }
    let now = Local::now();
    let filename = format!(
        "command.{:04}{:02}{:02}.log",
        now.year(),
        now.month(),
        now.day()
    );
    let path = dir.join(filename);
    let mut f = OpenOptions::new().create(true).append(true).open(&path)?;
    let ts = format_bracket_timestamp(now);
    let line = format!("{} {{{}}} : {}\n", ts, email, command);
    f.write_all(line.as_bytes())?;
    Ok(())
}

fn format_bracket_timestamp(now: chrono::DateTime<Local>) -> String {
    let frac5 = now.nanosecond() / 10_000; // 5-digit fractional
    format!(
        "[{:04}-{:02}-{:02}:{:02}:{:02}:{:05}]",
        now.year(),
        now.month(),
        now.day(),
        now.hour(),
        now.minute(),
        frac5
    )
}

pub fn write_audit_log(command: &str, requester: &str, req_time_iso: &str, node: &str, status: &str) -> Result<()> {
    write_audit_log_to(Path::new("./log"), command, requester, req_time_iso, node, status)
}

pub fn write_audit_log_to(path: &Path, command: &str, requester: &str, req_time_iso: &str, node: &str, status: &str) -> Result<()> {
    let mut f = OpenOptions::new().create(true).append(true).open(path)?;
    let line = format!("{} , {} , {} , {} , {}\n", command, requester, req_time_iso, node, status);
    f.write_all(line.as_bytes())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;
    use std::fs;
    use regex::Regex;

    #[test]
    fn logs_written() {
        let dir = tempdir().unwrap();
        write_command_log_with_dir(dir.path(), "user@example.com", "DIR").unwrap();
        let entries: Vec<_> = fs::read_dir(dir.path()).unwrap().collect();
        assert_eq!(entries.len(), 1);
        let p = entries[0].as_ref().unwrap().path();
        let s = fs::read_to_string(&p).unwrap();
        // Example: [2025-09-10:16:44:00000] {EMAIL} : DIR
        let re = Regex::new(r"^\[\d{4}-\d{2}-\d{2}:\d{2}:\d{2}:\d{5}\] \{.*?\} : DIR").unwrap();
        assert!(re.is_match(s.lines().next().unwrap()));

        let audit_path = dir.path().join("audit.log");
        write_audit_log_to(&audit_path, "cmd", "req", "2025-01-01T00:00:00Z", "node1", "success").unwrap();
        let s = fs::read_to_string(&audit_path).unwrap();
        assert!(s.contains("cmd , req , 2025-01-01T00:00:00Z , node1 , success"));
    }
}
