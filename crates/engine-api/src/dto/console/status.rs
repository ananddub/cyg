//! `GET /v2/console/status` and `GET /v2/console/route`: what the console's own
//! Status page and API explorer read.

use serde::Serialize;

/// The server as the console reports it. Every secret is absent — a status page
/// that prints the signing key is a status page nobody can leave open.
#[derive(Debug, Serialize)]
pub struct StatusResponse {
    pub name: String,
    pub version: String,
    pub api_port: u16,
    /// Seconds since the process started, not since the database did.
    pub uptime_secs: i64,
    /// Whether a database is configured at all; the counts below are zero when it
    /// is not, which is a different fact from "there are no accounts".
    pub database: bool,
    pub accounts: i64,
    pub console_users: i64,
    pub workers: usize,
    pub seat_ttl_secs: u64,
    pub rate_limit_rps: u32,
    pub rate_limit_burst: u32,
    pub player_token_expiry_secs: i64,
    pub console_token_expiry_secs: i64,
    /// Whether the Agones fleet integration is on.
    pub agones: bool,
}

/// One route the API explorer can call, read from the OpenAPI document the same
/// macros register — so the explorer cannot drift from the routes themselves.
#[derive(Debug, Serialize)]
pub struct RouteRow {
    pub method: String,
    pub path: String,
    pub tags: Vec<String>,
    pub summary: String,
}
