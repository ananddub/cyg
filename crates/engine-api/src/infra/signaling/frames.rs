//! The two message types that cross the hub boundary.

use crate::infra::seats::Seat;

/// Signaling from a browser, stamped with a seat the handler verified itself.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SignalIn {
    Offer {
        seat: Seat,
        sdp: String,
    },
    Candidate {
        seat: Seat,
        candidate: String,
        sdp_mid: Option<String>,
        sdp_index: Option<u32>,
    },
    Close {
        seat: Seat,
    },
}

/// Signaling bound for a browser.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SignalOut {
    Answer {
        sdp: String,
    },
    Candidate {
        candidate: String,
        sdp_mid: Option<String>,
        sdp_index: Option<u32>,
    },
    /// The `voice_ice_servers` blob from `config.toml`, relayed verbatim.
    IceServers {
        config: String,
    },
    Error {
        message: String,
    },
}
