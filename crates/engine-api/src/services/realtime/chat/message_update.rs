use super::*;

impl RealtimeChatService {
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
        let updated = crate::services::deps::q(&state).await?
            .update_channel_message(&UpdateChannelMessageParams {
                content: req.content,
                id,
                sender_id: s.uid,
            })
            .await
            .map_err(|e| match e {
                DbError::NotFound => V2Error::not_found("Channel message not found."),
                other => V2Error::from(other),
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
        let deleted = crate::services::deps::q(&state).await?
            .delete_channel_message(id, s.uid)
            .await
            .map_err(|e| match e {
                DbError::NotFound => V2Error::not_found("Channel message not found."),
                other => V2Error::from(other),
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
