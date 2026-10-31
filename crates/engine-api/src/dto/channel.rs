use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::repositories::models::Message;
use crate::services::social::{meta_str, ts, uid};
use crate::utils::channel::{MODE_DM, MODE_GROUP, MODE_ROOM, Stream};

#[derive(Debug, Deserialize)]
pub struct ListQuery {
    #[serde(default)]
    pub limit: Option<i64>,
    #[serde(default)]
    pub cursor: Option<String>,
    #[serde(default)]
    pub forward: Option<bool>,
}

#[derive(Debug, Default, Deserialize)]
pub struct SendRequest {
    #[serde(default)]
    pub content: Value,
}

#[derive(Debug, Default, Deserialize)]
pub struct UpdateRequest {
    #[serde(default)]
    pub content: Value,
}

#[derive(Debug, Serialize)]
pub struct MessageResponse {
    pub channel_id: String,
    pub message_id: String,
    pub code: i16,
    pub sender_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    pub content: String,
    pub create_time: String,
    pub update_time: String,
    pub persistent: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub room_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub group_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_id_one: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_id_two: Option<String>,
}

impl MessageResponse {
    pub fn new(m: &Message, stream: &Stream) -> Self {
        let (room_name, group_id, user_id_one, user_id_two) = match stream.mode {
            MODE_ROOM => (Some(stream.label.clone()), None, None, None),
            MODE_GROUP => (None, Some(uid(&stream.subject)), None, None),
            MODE_DM => (
                None,
                None,
                Some(uid(&stream.subject)),
                Some(uid(&stream.descriptor)),
            ),
            _ => (None, None, None, None),
        };
        MessageResponse {
            channel_id: stream.id(),
            message_id: uid(&m.id),
            code: m.code,
            sender_id: uid(&m.sender_id),
            username: (!m.username.is_empty()).then(|| m.username.clone()),
            content: meta_str(&m.content),
            create_time: ts(m.create_time),
            update_time: ts(m.update_time),
            persistent: true,
            room_name,
            group_id,
            user_id_one,
            user_id_two,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct MessageListResponse {
    pub messages: Vec<MessageResponse>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prev_cursor: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cacheable_cursor: Option<String>,
}
