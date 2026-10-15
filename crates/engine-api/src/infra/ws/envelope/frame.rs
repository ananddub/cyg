//! The outer frame: a `cid` plus the flattened message body.

use serde::{Deserialize, Serialize};

use super::message::EnvelopeMessage;

/// One WebSocket frame. `cid` is echoed on the reply so a client can correlate
/// it with the request that caused it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Envelope {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cid: Option<String>,
    #[serde(flatten)]
    pub message: EnvelopeMessage,
}
