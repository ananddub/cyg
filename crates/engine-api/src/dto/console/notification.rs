//! Notifications: the ones the console lists for one account, and the one it sends.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::repositories::models::Notification;

/// `GET /v2/console/notification?user_id=…&limit=…`. Notifications belong to a
/// player, so the console view is always scoped to one account.
#[derive(Debug, Default, Deserialize)]
pub struct NotificationQuery {
    #[serde(default)]
    pub user_id: String,
    #[serde(default)]
    pub limit: Option<i64>,
}

impl NotificationQuery {
    pub fn limit(&self) -> i64 {
        self.limit.unwrap_or(25).clamp(1, 100)
    }
}

/// One notification. `code` is the numeric code the client branches on; the
/// console shows it rather than trying to name it.
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

/// `POST /v2/console/notification` — the message the console sends a player. The
/// sender is the signed-in operator, taken from the token rather than the body: a
/// notification must never be able to claim someone else sent it.
#[derive(Debug, Default, Deserialize)]
pub struct SendNotificationRequest {
    #[serde(default)]
    pub user_id: String,
    #[serde(default)]
    pub subject: String,
    #[serde(default)]
    pub content: serde_json::Value,
    /// The numeric code the client branches on; `0` when the sender has none.
    #[serde(default)]
    pub code: i16,
}
