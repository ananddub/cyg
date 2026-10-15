//! The failure payload for `type:"error"` frames.

use serde::{Deserialize, Serialize};

/// A failure frame: a human-readable `message` and nothing else.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorMsg {
    pub message: String,
}
