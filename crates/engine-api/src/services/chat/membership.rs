//! Channel join/leave and the group membership guard.

use std::collections::HashMap;
use std::sync::Arc;

use uuid::Uuid;

use crate::ApiState;
use crate::error::V2Error;
use crate::infra::ws::envelope::{Envelope, EnvelopeMessage, Presence};
use crate::infra::ws::session;
use crate::services::presence;
use crate::utils::channel::{MODE_DM, MODE_GROUP, Stream, stream_of_channel};

use super::helpers::channel_json;
use super::state::{ChannelState, broadcast, channels, remove_member};

/// Group membership ranks at or below this may stand in a group channel —
/// mirrors `api_channel.rs`'s role check.
const ROLE_MEMBER: i16 = 2;

async fn assert_member(state: &ApiState, uid: Uuid, stream: &Stream) -> Result<(), V2Error> {
    match stream.mode {
        MODE_DM => {
            if uid != stream.subject && uid != stream.descriptor {
                return Err(V2Error::invalid_argument("Channel not found."));
            }
            Ok(())
        }
        MODE_GROUP => {
            let queries = crate::services::auth::q(state)?;
            match queries.get_group_edge_state(uid, stream.subject).await {
                Ok(code) if code <= ROLE_MEMBER => Ok(()),
                Ok(_) | Err(sqlx::Error::RowNotFound) => {
                    Err(V2Error::invalid_argument("Group not found."))
                }
                Err(e) => Err(V2Error::from_sqlx(e)),
            }
        }
        // Rooms are open; the plan leaves moderation to server runtime.
        _ => Ok(()),
    }
}

pub async fn join(
    state: &ApiState,
    s: &Arc<session::Session>,
    channel_id: &str,
    persistent: bool,
    cid: Option<String>,
) -> Result<(), V2Error> {
    let stream = stream_of_channel(channel_id)?;
    assert_member(state, s.uid, &stream).await?;
    let presence = presence::presence_of(s);
    let others: Vec<Presence> = {
        let mut all = channels().write().expect("channel registry lock");
        let entry = all
            .entry(channel_id.to_string())
            .or_insert_with(|| ChannelState {
                stream: stream.clone(),
                members: HashMap::new(),
            });
        let others = entry.members.values().cloned().collect::<Vec<_>>();
        entry.members.insert(s.sid, presence.clone());
        others
    };
    s.channels
        .write()
        .expect("channel lock")
        .insert(channel_id.to_string());
    s.send_message(
        cid,
        EnvelopeMessage::ChannelReply {
            channel: channel_json(&stream),
            presences: others.clone(),
        },
    );
    if !others.is_empty() || persistent {
        let event = Envelope {
            cid: None,
            message: EnvelopeMessage::ChannelPresenceEvent {
                channel_id: channel_id.to_string(),
                joins: vec![presence],
                leaves: Vec::new(),
            },
        };
        for p in others {
            if let Some(sid) = Uuid::parse_str(&p.session_id).ok() {
                if let Some(sess) = session::find(sid) {
                    sess.send(&event);
                }
            }
        }
    }
    Ok(())
}

pub fn leave(
    s: &Arc<session::Session>,
    channel_id: &str,
    cid: Option<String>,
) -> Result<(), V2Error> {
    let presence = remove_member(channel_id, s.sid);
    s.channels.write().expect("channel lock").remove(channel_id);
    if let Some(p) = presence {
        broadcast(
            channel_id,
            &Envelope {
                cid: None,
                message: EnvelopeMessage::ChannelPresenceEvent {
                    channel_id: channel_id.to_string(),
                    joins: Vec::new(),
                    leaves: vec![p],
                },
            },
            Some(s.sid),
        );
    }
    s.send_message(
        cid,
        EnvelopeMessage::ChannelLeave {
            channel_id: channel_id.to_string(),
        },
    );
    Ok(())
}
