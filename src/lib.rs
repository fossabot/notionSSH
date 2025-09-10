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
    use notion::{append_result_children, build_client, fetch_all_children, is_block_processed, lookup_user_email};
    use parser::parse_command_from_block;
    use util::extract_page_id;

    let cfg = load_config()?;
    let client = build_client(&cfg.api_key)?;
    let page_id = extract_page_id(&cfg.page_url)?;

    let blocks = fetch_all_children(&client, &page_id)?;
    let mut tasks = Vec::new();
    for b in blocks.iter() {
        if let Some(t) = parse_command_from_block(b) {
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

        write_command_log(&requester_id, &task.command, &task.created_time)?;
        write_audit_log(
            &task.command,
            &requester_email,
            &task.created_time,
            &node_name(),
            if status { "success" } else { "failed" },
        )?;

        append_result_children(&client, &task.block_id, &task.command, &out, &requester_email)?;
    }

    Ok(())
}
