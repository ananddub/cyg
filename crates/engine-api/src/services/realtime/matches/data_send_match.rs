use super::*;

impl RealtimeMatchService {
    pub fn data_send_match(
        &self,
        s: &Arc<Session>,
        raw_id: &str,
        op_code: i64,
        data: String,
    ) -> Result<(), V2Error> {
        let id = Uuid::parse_str(raw_id).map_err(|_| V2Error::not_found("match not found"))?;
        let (recipients, sender) = self
            .matches
            .read_sync(&id, |_, m| {
                if !m.members.contains_key(&s.sid) {
                    return Err(V2Error::permission_denied(
                        "must join the match before sending data",
                    ));
                }
                let recipients: Vec<Uuid> = m
                    .members
                    .values()
                    .map(|p| Uuid::parse_str(&p.user_id).unwrap_or_default())
                    .collect();
                Ok((recipients, self.presence_of(s)))
            })
            .ok_or_else(|| V2Error::not_found("match not found"))??;
        let event = Envelope {
            cid: None,
            message: EnvelopeMessage::MatchData {
                match_id: id.simple().to_string(),
                presence: Some(sender),
                op_code,
                data,
                reliable: true,
            },
        };
        self.notify(&recipients, s.sid, &event);
        Ok(())
    }
    pub fn signal_match(
        &self,
        s: &Arc<Session>,
        raw_id: &str,
        data: String,
        cid: Option<String>,
    ) -> Result<(), V2Error> {
        let id = Uuid::parse_str(raw_id).map_err(|_| V2Error::not_found("match not found"))?;
        let member_uids: Vec<Uuid> = self
            .matches
            .read_sync(&id, |_, m| {
                m.members
                    .values()
                    .map(|p| Uuid::parse_str(&p.user_id).unwrap_or_default())
                    .collect()
            })
            .ok_or_else(|| V2Error::not_found("match not found"))?;
        let event = Envelope {
            cid: None,
            message: EnvelopeMessage::MatchSignal {
                match_id: id.simple().to_string(),
                data,
            },
        };
        self.notify(&member_uids, s.sid, &event);
        s.send_message(
            cid,
            EnvelopeMessage::MatchSignal {
                match_id: id.simple().to_string(),
                data: String::new(),
            },
        );
        Ok(())
    }
    pub fn cleanup(&self, s: &Arc<Session>) {
        let mut ids: Vec<Uuid> = Vec::new();
        s.matches.iter_sync(|id| {
            ids.push(*id);
            true
        });
        for id in ids {
            let (member_ids, presence) = {
                let match_ref = self.matches.get_sync(&id);
                let Some(mut m) = match_ref else { continue };
                let presence = m.members.remove(&s.sid);
                let member_ids: Vec<Uuid> = m
                    .members
                    .keys()
                    .filter_map(|sid| session::find(*sid))
                    .map(|sess| sess.uid)
                    .collect();
                let empty = m.members.is_empty() && m.ephemeral;
                drop(m);
                if empty {
                    if let Some((_, removed)) = self.matches.remove_sync(&id) {
                        for tok in &removed.tokens {
                            self.tokens_to_match.remove_sync(tok);
                        }
                    }
                }
                (member_ids, presence)
            };
            if let Some(presence) = presence {
                let event = Envelope {
                    cid: None,
                    message: EnvelopeMessage::MatchPresenceEvent {
                        match_id: id.simple().to_string(),
                        joins: Vec::new(),
                        leaves: vec![presence],
                    },
                };
                for uid in member_ids {
                    session::send_to_user(uid, &event);
                }
            }
        }
    }
    pub fn terminate(&self, id: Uuid) -> bool {
        let members: Vec<Presence> = {
            let Some((_, m)) = self.matches.remove_sync(&id) else {
                return false;
            };
            for tok in &m.tokens {
                self.tokens_to_match.remove_sync(tok);
            }
            m.members.values().cloned().collect()
        };
        let event = Envelope {
            cid: None,
            message: EnvelopeMessage::MatchPresenceEvent {
                match_id: id.simple().to_string(),
                joins: Vec::new(),
                leaves: members.clone(),
            },
        };
        for presence in &members {
            let Ok(uid) = Uuid::parse_str(&presence.user_id) else {
                continue;
            };
            for sess in session::sessions_of(uid) {
                let _ = sess.matches.remove_sync(&id);
                sess.send(&event);
            }
        }
        true
    }
}
