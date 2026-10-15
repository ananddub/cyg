//! The social notification fan-out and the reserved notification codes.

use serde_json::{Value, json};
use uuid::Uuid;

use crate::repositories::db::Queries;
use crate::repositories::notification::CreateNotificationParams;

/// the reserved social notification codes (`server/core_notification.go`); negative so
/// a runtime-created notification (code 0+) can never collide with an engine one.
pub const NOTIFY_DM_REQUEST: i16 = -1;
pub const NOTIFY_FRIEND_REQUEST: i16 = -2;
pub const NOTIFY_FRIEND_ACCEPT: i16 = -3;
pub const NOTIFY_GROUP_ADD: i16 = -4;
pub const NOTIFY_GROUP_JOIN_REQUEST: i16 = -5;
pub const NOTIFY_USER_BANNED: i16 = -8;
pub const NOTIFY_FRIEND_REMOVE: i16 = -9;

/// A persistent notification about the caller, exactly as the friend core
/// writes it: `content` carrying the caller's username.
pub async fn notify(
    queries: &Queries,
    recipient: Uuid,
    sender: Uuid,
    sender_username: &str,
    subject: String,
    code: i16,
) {
    notify_with(
        queries,
        recipient,
        sender,
        subject,
        json!({ "username": sender_username }),
        code,
    )
    .await
}

/// The same row with an explicit content object, for the group events that carry
/// `{"group_id":…, "username":…}`. A store failure is logged, never propagated.
pub async fn notify_with(
    queries: &Queries,
    recipient: Uuid,
    sender: Uuid,
    subject: String,
    content: Value,
    code: i16,
) {
    let _ = queries
        .create_notification(&CreateNotificationParams {
            id: Uuid::new_v4(),
            user_id: recipient,
            subject,
            content,
            sender_id: sender,
            code,
        })
        .await;
}
