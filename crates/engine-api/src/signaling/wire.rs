//! The wire protocol: one JSON object per WS text frame.
//!
//! ```text
//! C→S  {"type":"offer","sdp":"v=0\r\n…"}
//!      {"type":"candidate","candidate":"…","sdpMid":"0","sdpMLineIndex":0}
//!      {"type":"close"}
//!
//! S→C  {"type":"ice","config":[{"urls":["stun:…"]}]}
//!      {"type":"answer","sdp":"v=0\r\n…"}
//!      {"type":"candidate","candidate":"…","sdpMid":"0","sdpMLineIndex":0}
//!      {"type":"error","message":"…"}
//! ```

use serde::{Deserialize, Serialize};

use super::frames::SignalOut;

/// What a browser sends. `type` is the tag; absent fields are a protocol error
/// reported by name, not a blanket deserialize failure.
#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub(super) enum ClientFrame {
    Offer { sdp: String },
    Candidate {
        candidate: String,
        #[serde(rename = "sdpMid")]
        sdp_mid: Option<String>,
        // The WebRTC-native spelling first (`sdpMLineIndex`, what the SDK and
        // `RTCIceCandidateInit` produce); `sdpIndex` stays accepted so the
        // shape documented in earlier drafts does not break mid-rollout.
        #[serde(rename = "sdpMLineIndex", alias = "sdpIndex")]
        sdp_index: Option<u32>,
    },
    Close,
}

/// What this plane writes back. `ice.config` is emitted as JSON rather than as
/// a quoted string, so a page can read `msg.config` directly.
#[derive(Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub(super) enum ServerFrame {
    Ice { config: serde_json::Value },
    Answer { sdp: String },
    Candidate {
        candidate: String,
        #[serde(rename = "sdpMid", skip_serializing_if = "Option::is_none")]
        sdp_mid: Option<String>,
        #[serde(rename = "sdpMLineIndex", skip_serializing_if = "Option::is_none")]
        sdp_index: Option<u32>,
    },
    Error { message: String },
}

impl From<SignalOut> for ServerFrame {
    fn from(msg: SignalOut) -> ServerFrame {
        match msg {
            SignalOut::Answer { sdp } => ServerFrame::Answer { sdp },
            SignalOut::Candidate { candidate, sdp_mid, sdp_index } => {
                ServerFrame::Candidate { candidate, sdp_mid, sdp_index }
            }
            SignalOut::IceServers { config } => ServerFrame::Ice { config: super::frames::parse_ice(&config) },
            SignalOut::Error { message } => ServerFrame::Error { message },
        }
    }
}
