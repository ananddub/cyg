//! `channel_message_send`: persist (unless ephemeral), fan out, ack.

use std::sync::Arc;

use chrono::Utc;
use serde_json::Value;
use uuid::Uuid;

use crate::ApiState;
use crate::error::V2Error;
use crate::infra::ws::envelope::{ChannelMessage, ChannelMessageAck, Envelope, EnvelopeMessage};
use crate::infra::ws::session;
use crate::repositories::channel::InsertChannelMessageParams;

use super::helpers::{content_to_string, stream_of};
use super::state::broadcast;

pub async fn message_send(
    state: &ApiState,
    s: &Arc<session::Session>,
    channel_id: &str,
    content: Value,
    persist: Option<bool>,
    ephemeral: Option<bool>,
    cid: Option<String>,
) -> Result<(), V2Error> {
    let stream = match stream_of(channel_id) {
        Some(st) => st,
        None => {
            return Err(V2Error::not_found(
                "must join the channel before sending messages",
            ));
        }
    };
    let persistent = !ephemeral.unwrap_or(false) && persist.unwrap_or(true);
    let id = Uuid::new_v4();
    let now = Utc::now();
    let secs = now.timestamp();

    if persistent {
        let msg = crate::services::auth::q(state)?
            .insert_channel_message(&InsertChannelMessageParams {
                id,
                sender_id: s.uid,
                username: s.username.clone(),
                stream_mode: stream.mode,
                stream_subject: stream.subject,
                stream_descriptor: stream.descriptor,
                stream_label: stream.label.clone(),
                content: content.clone(),
            })
            .await?;
        // The row keeps the canonical write; its id/times drive the ack.
        let _ = msg;
    }

    let wire = content_to_string(&content);
    broadcast(
        channel_id,
        &Envelope {
            cid: None,
            message: EnvelopeMessage::ChannelMessage(ChannelMessage {
                id: id.simple().to_string(),
                channel: channel_id.to_string(),
                username: s.username.clone(),
                display_name: String::new(),
                content: wire,
                code: 0,
                create_time: secs,
                update_time: secs,
                persistent: if persistent { Some(true) } else { None },
            }),
        },
        Some(s.sid),
    );

    s.send_message(
        cid,
        EnvelopeMessage::ChannelMessageAck(ChannelMessageAck {
            channel_id: channel_id.to_string(),
            message_id: id.simple().to_string(),
            code: 0,
            create_time: secs,
            update_time: secs,
            username: Some(s.username.clone()),
        }),
    );
    Ok(())
}
