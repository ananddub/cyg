//! `channel_message_update`: persist the edit, fan out (code 1), ack.

use std::sync::Arc;

use serde_json::Value;
use uuid::Uuid;

use crate::ApiState;
use crate::error::V2Error;
use crate::infra::ws::envelope::{ChannelMessage, ChannelMessageAck, Envelope, EnvelopeMessage};
use crate::infra::ws::session;
use crate::repositories::channel::UpdateChannelMessageParams;

use super::helpers::{stream_of, uid_simple};
use super::state::broadcast;

pub async fn message_update(
    state: &ApiState,
    s: &Arc<session::Session>,
    channel_id: &str,
    raw_message_id: &str,
    content: Value,
    cid: Option<String>,
) -> Result<(), V2Error> {
    if stream_of(channel_id).is_none() {
        return Err(V2Error::not_found(
            "must join the channel before updating messages",
        ));
    }
    let id = Uuid::parse_str(raw_message_id)
        .map_err(|_| V2Error::invalid_argument("Invalid message ID."))?;
    let msg = crate::services::auth::q(state)?
        .update_channel_message(&UpdateChannelMessageParams {
            content,
            id,
            sender_id: s.uid,
        })
        .await
        .map_err(|e| match e {
            sqlx::Error::RowNotFound => V2Error::not_found("Channel message not found."),
            other => V2Error::from_sqlx(other),
        })?;
    broadcast(
        channel_id,
        &Envelope {
            cid: None,
            message: EnvelopeMessage::ChannelMessage(ChannelMessage {
                id: uid_simple(&msg.id),
                channel: channel_id.to_string(),
                username: s.username.clone(),
                display_name: String::new(),
                content: crate::services::social::meta_str(&msg.content),
                code: 1,
                create_time: msg.create_time.timestamp(),
                update_time: msg.update_time.timestamp(),
                persistent: Some(true),
            }),
        },
        Some(s.sid),
    );
    s.send_message(
        cid,
        EnvelopeMessage::ChannelMessageAck(ChannelMessageAck {
            channel_id: channel_id.to_string(),
            message_id: msg.id.simple().to_string(),
            code: 1,
            create_time: msg.create_time.timestamp(),
            update_time: msg.update_time.timestamp(),
            username: Some(s.username.clone()),
        }),
    );
    Ok(())
}
