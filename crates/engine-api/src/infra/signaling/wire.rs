use serde::{Deserialize, Serialize};

use super::frames::SignalOut;

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub(super) enum ClientFrame {
    Offer {
        sdp: String,
    },
    Candidate {
        candidate: String,
        #[serde(rename = "sdpMid")]
        sdp_mid: Option<String>,
        #[serde(rename = "sdpMLineIndex", alias = "sdpIndex")]
        sdp_index: Option<u32>,
    },
    Close,
}

#[derive(Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub(super) enum ServerFrame {
    Ice {
        config: serde_json::Value,
    },
    Answer {
        sdp: String,
    },
    Candidate {
        candidate: String,
        #[serde(rename = "sdpMid", skip_serializing_if = "Option::is_none")]
        sdp_mid: Option<String>,
        #[serde(rename = "sdpMLineIndex", skip_serializing_if = "Option::is_none")]
        sdp_index: Option<u32>,
    },
    Error {
        message: String,
    },
}

impl From<SignalOut> for ServerFrame {
    fn from(msg: SignalOut) -> ServerFrame {
        match msg {
            SignalOut::Answer { sdp } => ServerFrame::Answer { sdp },
            SignalOut::Candidate {
                candidate,
                sdp_mid,
                sdp_index,
            } => ServerFrame::Candidate {
                candidate,
                sdp_mid,
                sdp_index,
            },
            SignalOut::IceServers { config } => ServerFrame::Ice {
                config: crate::utils::signaling::parse_ice(&config),
            },
            SignalOut::Error { message } => ServerFrame::Error { message },
        }
    }
}
