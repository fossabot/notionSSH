pub mod config;
pub mod executor;
pub mod logger;
pub mod model;
pub mod notion;
pub mod parser;
pub mod util;

use anyhow::Result;

pub fn run() -> Result<()> {
    use config::load_config;
    use executor::execute_command;
    use logger::{node_name, write_audit_log, write_command_log};
    use notion::{
        append_result_children, build_client, ensure_status_block, fetch_all_children,
        is_block_processed, lookup_user_email, update_status_block, STATUS_MARKER,
    };
    use parser::parse_command_from_block;
    use util::extract_page_id;

    let cfg = load_config()?;
    let client = build_client(&cfg.api_key)?;
    let page_id = extract_page_id(&cfg.page_url)?;

    // Ensure status block exists and set waiting status
    let status_id = ensure_status_block(&client, &page_id)?;
    let waiting_text = "[*] NotionSSH is Loading - waiting for commands. Press Ctrl+C to stop. # notionSSH-status";
    let _ = update_status_block(&client, &status_id, waiting_text);
    println!("Started polling Notion for commands. Press Ctrl+C to stop.");

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

        if tasks.is_empty() {
            let _ = update_status_block(&client, &status_id, waiting_text);
        } else {
            let count = tasks.len();
            let _ = update_status_block(
                &client,
                &status_id,
                &format!("[*] NotionSSH is Processing {} command(s)... # {}", count, STATUS_MARKER),
            );
            for task in tasks {
                let requester_id = task
                    .created_by_id
                    .clone()
                    .unwrap_or_else(|| "unknown".to_string());
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
                    &node_name(),
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
            let _ = update_status_block(&client, &status_id, waiting_text);
        }

        std::thread::sleep(std::time::Duration::from_secs(5));
    }
}
