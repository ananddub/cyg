use serde::{Deserialize, Serialize};

use super::message::EnvelopeMessage;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Envelope {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cid: Option<String>,
    #[serde(flatten)]
    pub message: EnvelopeMessage,
}
