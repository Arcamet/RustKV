#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatsSnapshot {
    pub uptime_ms: u64,
    pub active_connections: u64,
    pub total_connections: u64,
    pub total_requests: u64,
    pub total_errors: u64,
    pub get_commands: u64,
    pub set_commands: u64,
    pub delete_commands: u64,
    pub exists_commands: u64,
    pub stats_commands: u64,
    pub key_count: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Response {
    Ok,
    Value(Option<Vec<u8>>),
    Boolean(bool),
    Stats(StatsSnapshot),
    Error { code: u16, message: String },
}
