use super::*;

impl RealtimeChatService {
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
            let _ = crate::services::deps::q(&state)
                .await?
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
}
