use anyhow::{anyhow, Result};
use std::process::Command;

pub fn execute_command(cmd: &str) -> Result<(String, bool)> {
    #[cfg(target_os = "windows")]
    {
        let output = Command::new("cmd").args(["/C", cmd]).output()?;
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
        return Ok((combined, output.status.success()));
    }

    #[cfg(not(target_os = "windows"))]
    {
        let mut attempts: Vec<(String, Vec<String>)> = Vec::new();
        #[cfg(target_os = "linux")]
        {
            if let Ok(shell) = std::env::var("SHELL") {
                attempts.push((shell, vec!["-lc".into(), cmd.to_string()]));
            }
            attempts.push(("bash".into(), vec!["-lc".into(), cmd.to_string()]));
            attempts.push(("sh".into(), vec!["-lc".into(), cmd.to_string()]));
        }
        #[cfg(not(target_os = "linux"))]
        {
            attempts.push(("sh".into(), vec!["-lc".into(), cmd.to_string()]));
        }

        let mut last_err: Option<anyhow::Error> = None;
        for (prog, args) in attempts {
            match Command::new(&prog).args(&args).output() {
                Ok(out) => {
                    let mut combined = String::new();
                    if !out.stdout.is_empty() {
                        combined.push_str(&String::from_utf8_lossy(&out.stdout));
                    }
                    if !out.stderr.is_empty() {
                        if !combined.is_empty() { combined.push_str("\n"); }
                        combined.push_str(&String::from_utf8_lossy(&out.stderr));
                    }
                    let max = 16_000;
                    if combined.len() > max {
                        combined = combined[..max].to_string();
                        combined.push_str("\n... [truncated]\n");
                    }
                    return Ok((combined, out.status.success()));
                }
                Err(e) => {
                    last_err = Some(e.into());
                }
            }
        }
        Err(anyhow!(
            "Failed to run command with available shells (last error: {:?})",
            last_err
        ))
    }
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
