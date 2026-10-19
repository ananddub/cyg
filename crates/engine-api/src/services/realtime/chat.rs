//! Realtime Chat Service (Channels, Direct Messages & Streams) in a single consolidated file.
//!
//! All state and methods are fully encapsulated on the `RealtimeChatService` struct as lock-free `FastMap`s,
//! injected as a singleton via `auto_di` with zero free functions, statics, or mutexes.

use std::collections::HashMap;
use std::sync::Arc;
use auto_di::singleton;
use auto_ws::{Conn, Msg};
use chrono::Utc;
use serde_json::{Value, json};
use uuid::Uuid;

use crate::ApiState;
use crate::error::V2Error;
use crate::infra::ws::envelope::{
    ChannelMessage, ChannelMessageAck, Envelope, EnvelopeMessage, Presence,
};
use crate::infra::ws::session::{self, Session};
use crate::repositories::channel::{InsertChannelMessageParams, UpdateChannelMessageParams};
use crate::controllers::realtime::messages::*;
use crate::controllers::realtime::{Dispatched, session_of, state_of};
use crate::utils::channel::{MODE_DM, MODE_GROUP, Stream, stream_of_channel};
use super::{FastMap, new_fast_map};

const ROLE_MEMBER: i16 = 2;

pub struct ChannelState {
    pub stream: Stream,
    pub members: HashMap<Uuid /*sid*/, Presence>,
}

pub struct RealtimeChatService {
    pub channels: FastMap<String, ChannelState>,
}

#[singleton]
impl RealtimeChatService {
    pub fn new() -> Self {
        Self {
            channels: new_fast_map(),
        }
    }
}

impl RealtimeChatService {
    // --- Private Helper Methods ---

    fn channel_json(&self, stream: &Stream) -> Value {
        let id = crate::utils::channel::channel_of_stream(
            stream.mode,
            stream.subject,
            stream.descriptor,
            &stream.label,
        );
        let mut obj = json!({ "id": id });
        match stream.mode {
            MODE_DM => {
                obj["user_one_id"] = json!(stream.subject.to_string());
                obj["user_two_id"] = json!(stream.descriptor.to_string());
            }
            MODE_GROUP => obj["group_id"] = json!(stream.subject.to_string()),
            _ => obj["room_name"] = json!(stream.label),
        }
        obj
    }

    fn content_to_string(&self, content: &Value) -> String {
        match content {
            Value::String(s) => s.clone(),
            other => other.to_string(),
        }
    }

    fn presence_of(&self, s: &Session) -> Presence {
        Presence {
            user_id: s.uid.simple().to_string(),
            session_id: s.sid.simple().to_string(),
            username: s.username.clone(),
            node: None,
        }
    }

    async fn assert_member(&self, state: &ApiState, uid: Uuid, stream: &Stream) -> Result<(), V2Error> {
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
            _ => Ok(()),
        }
    }

    // --- Channel Management ---

    pub fn remove_member(&self, channel_id: &str, sid: Uuid) -> Option<Presence> {
        let mut channel_ref = self.channels.get_sync(channel_id);
        let left = match channel_ref.as_mut() {
            Some(c) => c.members.remove(&sid),
            None => return None,
        };
        let is_empty = channel_ref.as_ref().map(|c| c.members.is_empty()).unwrap_or(false);
        drop(channel_ref);
        if is_empty {
            self.channels.remove_sync(channel_id);
        }
        left
    }

    pub fn broadcast(&self, channel_id: &str, env: &Envelope, skip: Option<Uuid>) {
        self.channels.read_sync(channel_id, |_, c| {
            for sid in c.members.keys() {
                if Some(*sid) != skip {
                    if let Some(sess) = session::find(*sid) {
                        sess.send(env);
                    }
                }
            }
        });
    }

    pub fn stream_of(&self, channel_id: &str) -> Option<Stream> {
        self.channels.read_sync(channel_id, |_, c| c.stream.clone())
    }

    pub fn cleanup(&self, s: &Arc<Session>) {
        let mut joined: Vec<String> = Vec::new();
        s.channels.iter_sync(|c| {
            joined.push(c.clone());
            true
        });
        for channel_id in joined {
            if let Some(p) = self.remove_member(&channel_id, s.sid) {
                self.broadcast(
                    &channel_id,
                    &Envelope {
                        cid: None,
                        message: EnvelopeMessage::ChannelPresenceEvent {
                            channel_id: channel_id.clone(),
                            joins: Vec::new(),
                            leaves: vec![p],
                        },
                    },
                    None,
                );
            }
            let _ = s.channels.remove_sync(&channel_id);
        }
    }

    // --- Realtime WebSocket Event Handlers ---

    pub async fn join(&self, conn: &Conn, msg: &Msg) -> Dispatched {
        let req: ChannelJoin = msg.payload()?;
        let state = state_of(conn);
        let s = session_of(conn);
        let stream = stream_of_channel(&req.channel_id)?;
        self.assert_member(&state, s.uid, &stream).await?;
        let presence = self.presence_of(&s);
        let others: Vec<Presence> = {
            let mut entry = self
                .channels
                .entry_sync(req.channel_id.to_string())
                .or_insert_with(|| ChannelState {
                    stream: stream.clone(),
                    members: HashMap::new(),
                });
            let others = entry.members.values().cloned().collect::<Vec<_>>();
            entry.members.insert(s.sid, presence.clone());
            others
        };
        let _ = s.channels.insert_sync(req.channel_id.to_string());
        s.send_message(
            msg.cid().map(str::to_string),
            EnvelopeMessage::ChannelReply {
                channel: self.channel_json(&stream),
                presences: others.clone(),
            },
        );
        if !others.is_empty() || req.persistent {
            let event = Envelope {
                cid: None,
                message: EnvelopeMessage::ChannelPresenceEvent {
                    channel_id: req.channel_id.to_string(),
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
        Ok(None)
    }

    pub async fn leave(&self, conn: &Conn, msg: &Msg) -> Dispatched {
        let req: ChannelLeave = msg.payload()?;
        let s = session_of(conn);
        let presence = self.remove_member(&req.channel_id, s.sid);
        let _ = s.channels.remove_sync(&req.channel_id);
        if let Some(p) = presence {
            self.broadcast(
                &req.channel_id,
                &Envelope {
                    cid: None,
                    message: EnvelopeMessage::ChannelPresenceEvent {
                        channel_id: req.channel_id.to_string(),
                        joins: Vec::new(),
                        leaves: vec![p],
                    },
                },
                Some(s.sid),
            );
        }
        s.send_message(
            msg.cid().map(str::to_string),
            EnvelopeMessage::ChannelLeave {
                channel_id: req.channel_id.to_string(),
            },
        );
        Ok(None)
    }

    pub async fn message_send(&self, conn: &Conn, msg: &Msg) -> Dispatched {
        let req: ChannelMessageSend = msg.payload()?;
        let state = state_of(conn);
        let s = session_of(conn);
        let stream = match self.stream_of(&req.channel_id) {
            Some(st) => st,
            None => {
                return Err(V2Error::not_found(
                    "must join the channel before sending messages",
                ));
            }
        };
        let persistent = !req.ephemeral.unwrap_or(false) && req.persist.unwrap_or(true);
        let id = Uuid::new_v4();
        let now = Utc::now();
        let secs = now.timestamp();

        if persistent {
            let _ = crate::services::auth::q(&state)?
                .insert_channel_message(&InsertChannelMessageParams {
                    id,
                    sender_id: s.uid,
                    username: s.username.clone(),
                    stream_mode: stream.mode,
                    stream_subject: stream.subject,
                    stream_descriptor: stream.descriptor,
                    stream_label: stream.label.clone(),
                    content: req.content.clone(),
                })
                .await?;
        }

        let wire = self.content_to_string(&req.content);
        self.broadcast(
            &req.channel_id,
            &Envelope {
                cid: None,
                message: EnvelopeMessage::ChannelMessage(ChannelMessage {
                    id: id.simple().to_string(),
                    channel: req.channel_id.to_string(),
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
            msg.cid().map(str::to_string),
            EnvelopeMessage::ChannelMessageAck(ChannelMessageAck {
                channel_id: req.channel_id.to_string(),
                message_id: id.simple().to_string(),
                code: 0,
                create_time: secs,
                update_time: secs,
                username: Some(s.username.clone()),
            }),
        );
        Ok(None)
    }

    pub async fn message_update(&self, conn: &Conn, msg: &Msg) -> Dispatched {
        let req: ChannelMessageUpdate = msg.payload()?;
        let state = state_of(conn);
        let s = session_of(conn);
        if self.stream_of(&req.channel_id).is_none() {
            return Err(V2Error::not_found(
                "must join the channel before updating messages",
            ));
        }
        let id = Uuid::parse_str(&req.message_id)
            .map_err(|_| V2Error::invalid_argument("Invalid message ID."))?;
        let updated = crate::services::auth::q(&state)?
            .update_channel_message(&UpdateChannelMessageParams {
                content: req.content,
                id,
                sender_id: s.uid,
            })
            .await
            .map_err(|e| match e {
                sqlx::Error::RowNotFound => V2Error::not_found("Channel message not found."),
                other => V2Error::from_sqlx(other),
            })?;
        self.broadcast(
            &req.channel_id,
            &Envelope {
                cid: None,
                message: EnvelopeMessage::ChannelMessage(ChannelMessage {
                    id: updated.id.simple().to_string(),
                    channel: req.channel_id.to_string(),
                    username: s.username.clone(),
                    display_name: String::new(),
                    content: crate::services::social::meta_str(&updated.content),
                    code: 1,
                    create_time: updated.create_time.timestamp(),
                    update_time: updated.update_time.timestamp(),
                    persistent: Some(true),
                }),
            },
            Some(s.sid),
        );
        s.send_message(
            msg.cid().map(str::to_string),
            EnvelopeMessage::ChannelMessageAck(ChannelMessageAck {
                channel_id: req.channel_id.to_string(),
                message_id: updated.id.simple().to_string(),
                code: 1,
                create_time: updated.create_time.timestamp(),
                update_time: updated.update_time.timestamp(),
                username: Some(s.username.clone()),
            }),
        );
        Ok(None)
    }

    pub async fn message_remove(&self, conn: &Conn, msg: &Msg) -> Dispatched {
        let req: ChannelMessageRemove = msg.payload()?;
        let state = state_of(conn);
        let s = session_of(conn);
        if self.stream_of(&req.channel_id).is_none() {
            return Err(V2Error::not_found(
                "must join the channel before removing messages",
            ));
        }
        let id = Uuid::parse_str(&req.message_id)
            .map_err(|_| V2Error::invalid_argument("Invalid message ID."))?;
        let deleted = crate::services::auth::q(&state)?
            .delete_channel_message(id, s.uid)
            .await
            .map_err(|e| match e {
                sqlx::Error::RowNotFound => V2Error::not_found("Channel message not found."),
                other => V2Error::from_sqlx(other),
            })?;
        self.broadcast(
            &req.channel_id,
            &Envelope {
                cid: None,
                message: EnvelopeMessage::ChannelMessage(ChannelMessage {
                    id: deleted.id.simple().to_string(),
                    channel: req.channel_id.to_string(),
                    username: s.username.clone(),
                    display_name: String::new(),
                    content: "{}".into(),
                    code: 2,
                    create_time: deleted.create_time.timestamp(),
                    update_time: deleted.update_time.timestamp(),
                    persistent: Some(true),
                }),
            },
            Some(s.sid),
        );
        s.send_message(
            msg.cid().map(str::to_string),
            EnvelopeMessage::ChannelMessageAck(ChannelMessageAck {
                channel_id: req.channel_id.to_string(),
                message_id: deleted.id.simple().to_string(),
                code: 2,
                create_time: deleted.create_time.timestamp(),
                update_time: deleted.update_time.timestamp(),
                username: Some(s.username.clone()),
            }),
        );
        Ok(None)
    }

    pub async fn stream_data(&self, conn: &Conn, msg: &Msg) -> Dispatched {
        let req: StreamData = msg.payload()?;
        let s = session_of(conn);
        if self.stream_of(&req.stream_id).is_none() {
            return Err(V2Error::not_found(
                "must join the stream before sending data",
            ));
        }
        self.broadcast(
            &req.stream_id,
            &Envelope {
                cid: None,
                message: EnvelopeMessage::StreamData {
                    stream_id: req.stream_id.to_string(),
                    data: req.data.clone(),
                    reliable: req.reliable,
                },
            },
            Some(s.sid),
        );
        if req.reliable {
            s.send_message(
                msg.cid().map(str::to_string),
                EnvelopeMessage::StreamData {
                    stream_id: req.stream_id.to_string(),
                    data: req.data,
                    reliable: req.reliable,
                },
            );
        }
        Ok(None)
    }
}
