use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct StatusResponse {
    pub name: String,
    pub version: String,
    pub api_port: u16,
    pub uptime_secs: i64,
    pub database: bool,
    pub accounts: i64,
    pub console_users: i64,
    pub workers: usize,
    pub seat_ttl_secs: u64,
    pub rate_limit_rps: u32,
    pub rate_limit_burst: u32,
    pub player_token_expiry_secs: i64,
    pub console_token_expiry_secs: i64,
    pub agones: bool,
}

#[derive(Debug, Serialize)]
pub struct RouteRow {
    pub method: String,
    pub path: String,
    pub tags: Vec<String>,
    pub summary: String,
}
