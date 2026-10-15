//! Realtime presence records shared by the match, party, channel and status planes.

use serde::{Deserialize, Serialize};

/// A realtime presence: one session of one user.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Presence {
    pub user_id: String,
    pub session_id: String,
    pub username: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub node: Option<String>,
}

/// Status plane presence: a followed user plus their free-form status string.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatusPresence {
    pub user_id: String,
    pub session_id: String,
    pub username: String,
    #[serde(default)]
    pub status: String,
}
