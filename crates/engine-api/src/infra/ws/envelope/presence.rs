use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Presence {
    pub user_id: String,
    pub session_id: String,
    pub username: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub node: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatusPresence {
    pub user_id: String,
    pub session_id: String,
    pub username: String,
    #[serde(default)]
    pub status: String,
}
