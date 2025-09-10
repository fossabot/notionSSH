use serde::Deserialize;
use serde_json::Value;

#[derive(Debug, Clone)]
pub struct CommandTask {
    pub block_id: String,
    pub command: String,
    pub created_time: String,
    pub created_by_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct PaginatedBlocks {
    pub results: Vec<Value>,
    pub next_cursor: Option<String>,
    pub has_more: bool,
}

