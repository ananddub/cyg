use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::repositories::models::Notification;

#[derive(Debug, Default, Deserialize)]
pub struct NotificationQuery {
    #[serde(default)]
    pub user_id: String,
    #[serde(default)]
    pub limit: Option<i64>,
    #[serde(default)]
    pub game_id: Option<Uuid>,
}

impl NotificationQuery {
    pub fn limit(&self) -> i64 {
        self.limit.unwrap_or(25).clamp(1, 100)
    }
}

#[derive(Debug, Serialize)]
pub struct NotificationRow {
    pub id: Uuid,
    pub user_id: Uuid,
    pub subject: String,
    pub content: serde_json::Value,
    pub code: i16,
    pub sender_id: Uuid,
    pub create_time: DateTime<Utc>,
}

impl From<Notification> for NotificationRow {
    fn from(n: Notification) -> Self {
        Self {
            id: n.id,
            user_id: n.user_id,
            subject: n.subject,
            content: n.content,
            code: n.code,
            sender_id: n.sender_id,
            create_time: n.create_time,
        }
    }
}

#[derive(Debug, Default, Deserialize)]
pub struct SendNotificationRequest {
    #[serde(default)]
    pub user_id: String,
    #[serde(default)]
    pub subject: String,
    #[serde(default)]
    pub content: serde_json::Value,
    #[serde(default)]
    pub code: i16,
}
