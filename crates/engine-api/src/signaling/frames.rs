//! The two message types that cross the hub boundary.

use crate::seats::Seat;

/// Signaling from a browser, after this handler stamped it with a seat it
/// verified itself.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SignalIn {
    Offer { seat: Seat, sdp: String },
    Candidate {
        seat: Seat,
        candidate: String,
        sdp_mid: Option<String>,
        sdp_index: Option<u32>,
    },
    Close { seat: Seat },
}

/// Signaling bound for a browser.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SignalOut {
    Answer { sdp: String },
    Candidate {
        candidate: String,
        sdp_mid: Option<String>,
        sdp_index: Option<u32>,
    },
    /// The `voice_ice_servers` blob from `config.toml`, relayed verbatim.
    IceServers { config: String },
    Error { message: String },
}

/// Malformed ICE text becomes an empty list rather than a failed connection:
/// the config is operator-supplied, and LAN-only still works.
pub fn parse_ice(config: &str) -> serde_json::Value {
    serde_json::from_str(config).unwrap_or_else(|_| serde_json::Value::Array(vec![]))
}
