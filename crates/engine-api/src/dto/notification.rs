//! `/v2/notification*` — the `apiNotification` shape and its list envelope.
//!
//! Shared with the realtime layer: the socket's `notifications_list` reply carries
//! exactly these objects, which is why the type is `Deserialize` too.

use serde::{Deserialize, Serialize};

use crate::repositories::models::Notification;
use crate::services::social::{meta_str, ts, uid};

/// `ListNotifications`' query.
#[derive(Debug, Deserialize)]
pub struct ListQuery {
    #[serde(default)]
    pub limit: Option<i64>,
    /// The gateway's name for the paging cursor, which accepts the JSON spelling
    /// and the field name itself.
    #[serde(default, rename = "cacheableCursor", alias = "cacheable_cursor")]
    pub cacheable_cursor: Option<String>,
    /// Only notifications raised by this account.
    #[serde(default)]
    pub sender_id: Option<String>,
    /// Only notifications carrying this category code.
    #[serde(default)]
    pub code: Option<i16>,
}

/// `DeleteNotifications`: a repeated field, or a comma-joined one, or both.
#[derive(Debug, Default, Deserialize)]
pub struct DeleteRequest {
    #[serde(default)]
    pub ids: Vec<String>,
}

/// `apiNotification`: `content` is a JSON string, `create_time` an ISO string,
/// and a system notification has no `sender_id` at all.
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

/// `ListNotifications`: the page, plus the cursor to resume at. An empty page
/// carries no `notifications` member at all, exactly as the gateway marshals it.
#[derive(Debug, Serialize, Deserialize, Default)]
pub struct NotificationListResponse {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub notifications: Vec<NotificationResponse>,
    pub cacheable_cursor: String,
}
