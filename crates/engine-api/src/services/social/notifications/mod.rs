use serde_json::{Value, json};
use uuid::Uuid;

use crate::ApiState;
use crate::repositories::notification::CreateNotificationParams;
use crate::services::deps::q;
use crate::services::notify::outbox;

pub const NOTIFY_DM_REQUEST: i16 = -1;
pub const NOTIFY_FRIEND_REQUEST: i16 = -2;
pub const NOTIFY_FRIEND_ACCEPT: i16 = -3;
pub const NOTIFY_GROUP_ADD: i16 = -4;
pub const NOTIFY_GROUP_JOIN_REQUEST: i16 = -5;
pub const NOTIFY_USER_BANNED: i16 = -8;
pub const NOTIFY_FRIEND_REMOVE: i16 = -9;

/// The name a preference and an email template are keyed by. Codes are what the
/// client protocol speaks, so they stay the row's field; this is the one place
/// that says which fact each number is.
pub fn kind_of(code: i16) -> &'static str {
    match code {
        NOTIFY_DM_REQUEST => "dm_request",
        NOTIFY_FRIEND_REQUEST => "friend_request",
        NOTIFY_FRIEND_ACCEPT => "friend_accept",
        NOTIFY_GROUP_ADD => "group_add",
        NOTIFY_GROUP_JOIN_REQUEST => "group_join_request",
        NOTIFY_USER_BANNED => "user_banned",
        NOTIFY_FRIEND_REMOVE => "friend_remove",
        _ => "notice",
    }
}

pub async fn notify(
    state: &ApiState,
    recipient: Uuid,
    sender: Uuid,
    sender_username: &str,
    subject: String,
    code: i16,
) {
    notify_with(
        state,
        recipient,
        sender,
        subject,
        json!({ "username": sender_username }),
        code,
    )
    .await
}

/// Writes the in-app row, then hands the outside world's copies to the outbox.
/// A notification that could not be written is not delivered anywhere either:
/// the inbox is the fact, and a message with no fact behind it is a bug.
pub async fn notify_with(
    state: &ApiState,
    recipient: Uuid,
    sender: Uuid,
    subject: String,
    content: Value,
    code: i16,
) {
    let Ok(queries) = q(state).await else {
        return;
    };
    let Ok(row) = queries
        .create_notification(&CreateNotificationParams {
            id: Uuid::new_v4(),
            user_id: recipient,
            subject,
            content,
            sender_id: sender,
            code,
        })
        .await
    else {
        return;
    };
    outbox::enqueue(state, &queries, &row).await;
}
