use anyhow::Result;
use chrono::{Datelike, Local};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::Path;

pub use crate::util::node_name;

pub fn write_command_log(user_id: &str, command: &str, req_time_iso: &str) -> Result<()> {
    let logs_dir = Path::new("./logs");
    write_command_log_with_dir(logs_dir, user_id, command, req_time_iso)
}

pub fn write_command_log_with_dir(dir: &Path, user_id: &str, command: &str, req_time_iso: &str) -> Result<()> {
    if !dir.exists() {
        fs::create_dir_all(dir)?;
    }
    let today = Local::now();
    let filename = format!(
        "command.{:04}{:02}{:02}.log",
        today.year(),
        today.month(),
        today.day()
    );
    let path = dir.join(filename);
    let mut f = OpenOptions::new().create(true).append(true).open(&path)?;
    let line = format!("[{}] ({}) : {}\n", req_time_iso, user_id, command);
    f.write_all(line.as_bytes())?;
    Ok(())
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

    #[test]
    fn logs_written() {
        let dir = tempdir().unwrap();
        write_command_log_with_dir(dir.path(), "u1", "echo hi", "2025-01-01T00:00:00Z").unwrap();
        let entries: Vec<_> = fs::read_dir(dir.path()).unwrap().collect();
        assert_eq!(entries.len(), 1);

        let audit_path = dir.path().join("audit.log");
        write_audit_log_to(&audit_path, "cmd", "req", "2025-01-01T00:00:00Z", "node1", "success").unwrap();
        let s = fs::read_to_string(&audit_path).unwrap();
        assert!(s.contains("cmd , req , 2025-01-01T00:00:00Z , node1 , success"));
    }
}
