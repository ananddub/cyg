//! Chat channel reply payloads.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelMessage {
    pub id: String,
    pub channel: String,
    pub username: String,
    #[serde(default)]
    pub display_name: String,
    pub content: String,
    #[serde(default)]
    pub code: i32,
    #[serde(default)]
    pub create_time: i64,
    #[serde(default)]
    pub update_time: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub persistent: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelMessageAck {
    pub channel_id: String,
    pub message_id: String,
    pub code: i32,
    #[serde(default)]
    pub create_time: i64,
    #[serde(default)]
    pub update_time: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
}
