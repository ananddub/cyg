use serde::Serialize;

use crate::error::SidecarError;
use crate::health::PingStats;
use crate::state::ServerState;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SidecarErrorInfo {
    pub kind: String,
    pub message: String,
}

impl SidecarErrorInfo {
    pub fn new(error: &SidecarError) -> SidecarErrorInfo {
        SidecarErrorInfo {
            kind: error.kind().as_str().to_string(),
            message: error.message(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgonesSnapshot {
    pub enabled: bool,
    pub addr: String,
    pub state: ServerState,
    pub state_raw: String,
    pub ready: bool,
    pub last_transition: String,
    pub last_error: Option<SidecarErrorInfo>,
    pub shutdown_sent: bool,
    pub stopped: bool,
    pub interval_ms: u64,
    pub backoff_ms: u64,
    pub uptime_ms: u64,
    #[serde(flatten)]
    pub pings: PingStats,
}

impl Default for AgonesSnapshot {
    fn default() -> AgonesSnapshot {
        AgonesSnapshot::disabled()
    }
}

impl AgonesSnapshot {
    pub fn disabled() -> AgonesSnapshot {
        AgonesSnapshot {
            enabled: false,
            addr: String::new(),
            state: ServerState::Unknown,
            state_raw: String::new(),
            ready: false,
            last_transition: String::new(),
            last_error: None,
            shutdown_sent: false,
            stopped: false,
            interval_ms: 0,
            backoff_ms: 0,
            uptime_ms: 0,
            pings: PingStats::default(),
        }
    }
}
