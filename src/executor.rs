use anyhow::{Context, Result};
use std::process::Command;

pub fn execute_command(cmd: &str) -> Result<(String, bool)> {
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
    let max = 16_000;
    if combined.len() > max {
        combined = combined[..max].to_string();
        combined.push_str("\n... [truncated]\n");
    }
    Ok((combined, output.status.success()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exec_echo_ok() {
        #[cfg(target_os = "windows")]
        let (out, ok) = execute_command("echo hello").unwrap();
        #[cfg(not(target_os = "windows"))]
        let (out, ok) = execute_command("echo hello").unwrap();
        assert!(ok);
        assert!(out.to_lowercase().contains("hello"));
    }

    #[test]
    fn exec_fail_status() {
        #[cfg(target_os = "windows")]
        let (_out, ok) = execute_command("nonexistent_command_zzz").unwrap();
        #[cfg(not(target_os = "windows"))]
        let (_out, ok) = execute_command("nonexistent_command_zzz").unwrap();
        assert!(!ok);
    }
}

