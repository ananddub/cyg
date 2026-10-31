use serde::{Deserialize, Serialize};

use crate::repositories::models::Notification;
use crate::services::social::{meta_str, ts, uid};

#[derive(Debug, Deserialize)]
pub struct ListQuery {
    #[serde(default)]
    pub limit: Option<i64>,
    #[serde(default, rename = "cacheableCursor", alias = "cacheable_cursor")]
    pub cacheable_cursor: Option<String>,
    #[serde(default)]
    pub sender_id: Option<String>,
    #[serde(default)]
    pub code: Option<i16>,
}

#[derive(Debug, Default, Deserialize)]
pub struct DeleteRequest {
    #[serde(default)]
    pub ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationResponse {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subject: Option<String>,
    pub content: String,
    pub code: i16,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sender_id: Option<String>,
    pub create_time: String,
    pub persistent: bool,
}

impl From<&Notification> for NotificationResponse {
    fn from(n: &Notification) -> Self {
        NotificationResponse {
            id: uid(&n.id),
            subject: (!n.subject.is_empty()).then(|| n.subject.clone()),
            content: meta_str(&n.content),
            code: n.code,
            sender_id: (!n.sender_id.is_nil()).then(|| uid(&n.sender_id)),
            create_time: ts(n.create_time),
            persistent: true,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct NotificationListResponse {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub notifications: Vec<NotificationResponse>,
    pub cacheable_cursor: String,
}
