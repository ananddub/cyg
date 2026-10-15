//! The wire protocol: one JSON object per WS text frame. Client `type`s are
//! `offer`, `candidate`, `close`; server `type`s `ice`, `answer`, `candidate`, `error`.

use serde::{Deserialize, Serialize};

use super::frames::SignalOut;

/// What a browser sends. `type` is the tag; an absent field is a protocol error
/// reported by name rather than a blanket deserialize failure.
#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub(super) enum ClientFrame {
    Offer { sdp: String },
    Candidate {
        candidate: String,
        #[serde(rename = "sdpMid")]
        sdp_mid: Option<String>,
        // `sdpMLineIndex` is the WebRTC-native spelling the SDK produces;
        // `sdpIndex` stays accepted so the shape documented in earlier drafts
        // does not break mid-rollout.
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
            SignalOut::IceServers { config } => ServerFrame::Ice { config: crate::utils::signaling::parse_ice(&config) },
            SignalOut::Error { message } => ServerFrame::Error { message },
        }
    }
}
