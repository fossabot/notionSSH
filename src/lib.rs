pub mod config;
pub mod executor;
pub mod logger;
pub mod model;
pub mod notion;
pub mod parser;
pub mod util;
pub mod verify;

use anyhow::Result;

pub fn run() -> Result<()> {
    use config::load_config;
    use executor::execute_command;
    use logger::{write_audit_log, write_command_log};
    use notion::{
        append_result_children, build_client, fetch_all_children, is_block_processed,
        lookup_user_email,
    };
    use parser::parse_command_from_block;
    use util::{extract_page_id, os_name};

    let enable_ca = if verify::saved_ca_pins_exist() {
        println!("[*] Using saved CA public key verification (.notionSSH/ca.json)");
        true
    } else {
        println!("Use CA public key fingerprint verification? [Y/N]");
        let mut input = String::new();
        let _ = std::io::stdin().read_line(&mut input);
        matches!(input.trim().to_uppercase().as_str(), "Y" | "YES")
    };

    println!("[*] Verifying Notion API Server...");
    if let Err(e) = verify::verify_notion_endpoint(enable_ca) {
        if enable_ca {
            eprintln!("[!] 영어로 인증 실패. 인증서 섹션을 참고하세요.");
        }
        eprintln!("[!] WARNING: Notion TLS/DoH verification failed: {e}");
        std::process::exit(2);
    } else if enable_ca && verify::ca_pins_configured() {
        println!("[*] Certificate verification passed.");
    }

    let cfg = load_config()?;
    let client = build_client(&cfg.api_key)?;
    let page_id = extract_page_id(&cfg.page_url)?;

    println!("[*] NotionSSH is Loading - waiting for commands. Press Ctrl+C to stop.");

    loop {
        // Scan for commands
        let mut tasks = Vec::new();
        match fetch_all_children(&client, &page_id) {
            Ok(blocks) => {
                for b in blocks.iter() {
                    if let Some(t) = parse_command_from_block(b) {
                        if is_block_processed(&client, &t.block_id).unwrap_or(false) {
                            continue;
                        }
                        tasks.push(t);
                    }
                }
            }
            Err(err) => {
                eprintln!("Failed to fetch children: {err:#}");
            }
        }

        if !tasks.is_empty() {
            for task in tasks {
                let requester_email = lookup_user_email(
                    &client,
                    task.created_by_id.as_deref().unwrap_or("")
                )
                .unwrap_or_else(|| "unknown".to_string());

                let (out, status) = execute_command(&task.command)?;

                write_command_log(&requester_email, &task.command)?;
                write_audit_log(
                    &task.command,
                    &requester_email,
                    &task.created_time,
                    &os_name(),
                    if status { "success" } else { "failed" },
                )?;

                append_result_children(
                    &client,
                    &task.block_id,
                    &task.command,
                    &out,
                    &requester_email,
                )?;
            }
        }

        std::thread::sleep(std::time::Duration::from_secs(1));
    }
}
