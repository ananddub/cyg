use super::*;

impl RealtimeChatService {
    pub(super) fn channel_json(&self, stream: &Stream) -> Value {
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
    pub(super) fn content_to_string(&self, content: &Value) -> String {
        match content {
            Value::String(s) => s.clone(),
            other => other.to_string(),
        }
    }
    pub(super) fn presence_of(&self, s: &Session) -> Presence {
        Presence {
            user_id: s.uid.simple().to_string(),
            session_id: s.sid.simple().to_string(),
            username: s.username.clone(),
            node: None,
        }
    }
    pub(super) async fn assert_member(
        &self,
        state: &ApiState,
        uid: Uuid,
        stream: &Stream,
    ) -> Result<(), V2Error> {
        match stream.mode {
            MODE_DM => {
                if uid != stream.subject && uid != stream.descriptor {
                    return Err(V2Error::invalid_argument("Channel not found."));
                }
                Ok(())
            }
            MODE_GROUP => {
                let queries = crate::services::deps::q(state).await?;
                match queries.get_group_edge_state(uid, stream.subject).await {
                    Ok(code) if code <= ROLE_MEMBER => Ok(()),
                    Ok(_) | Err(DbError::NotFound) => {
                        Err(V2Error::invalid_argument("Group not found."))
                    }
                    Err(e) => Err(V2Error::from(e)),
                }
            }
            _ => Ok(()),
        }
    }
    pub fn remove_member(&self, channel_id: &str, sid: Uuid) -> Option<Presence> {
        let mut channel_ref = self.channels.get_sync(channel_id);
        let left = match channel_ref.as_mut() {
            Some(c) => c.members.remove(&sid),
            None => return None,
        };
        let is_empty = channel_ref
            .as_ref()
            .map(|c| c.members.is_empty())
            .unwrap_or(false);
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
}
