use super::*;

impl RealtimePresenceService {
    pub(super) fn to_status_presence(
        &self,
        uid: Uuid,
        sid: Uuid,
        username: &str,
        status: &str,
    ) -> StatusPresence {
        StatusPresence {
            user_id: uid.simple().to_string(),
            session_id: sid.simple().to_string(),
            username: username.to_string(),
            status: status.to_string(),
        }
    }
    pub(super) fn followers_of(&self, target: Uuid) -> Vec<Arc<Session>> {
        let mut seen = Vec::new();
        for uid in session::online_users() {
            for s in session::sessions_of(uid) {
                if s.follows.contains_sync(&target) {
                    seen.push(s);
                }
            }
        }
        seen
    }
    pub fn online_for(&self, uids: &[Uuid]) -> Vec<StatusPresence> {
        uids.iter()
            .filter_map(|uid| {
                self.presences
                    .read_sync(uid, |_, by_sid| {
                        by_sid.iter().next().map(|(sid, (username, status))| {
                            self.to_status_presence(*uid, *sid, username, status)
                        })
                    })
                    .flatten()
            })
            .collect()
    }
    pub fn presence_of(&self, session: &Session) -> Presence {
        Presence {
            user_id: session.uid.simple().to_string(),
            session_id: session.sid.simple().to_string(),
            username: session.username.clone(),
            node: None,
        }
    }
    pub fn on_disconnect(&self, sid: Uuid) {
        let Some(session) = session::find(sid) else {
            return;
        };
        let gone = {
            let presence_ref = self.presences.get_sync(&session.uid);
            let Some(mut by_sid) = presence_ref else {
                return;
            };
            by_sid.get_mut().remove(&sid);
            if by_sid.is_empty() {
                let _ = by_sid.remove();
                true
            } else {
                false
            }
        };
        if gone {
            let leaves =
                vec![self.to_status_presence(session.uid, session.sid, &session.username, "")];
            for f in self.followers_of(session.uid) {
                f.send(&Envelope {
                    cid: None,
                    message: EnvelopeMessage::StatusPresenceEvent {
                        joins: Vec::new(),
                        leaves: leaves.clone(),
                    },
                });
            }
        }
    }
    pub async fn follow_users(
        &self,
        state: &ApiState,
        sid: Uuid,
        user_ids: Vec<String>,
        usernames: Vec<String>,
        cid: Option<String>,
    ) -> Result<(), V2Error> {
        let Some(session) = session::find(sid) else {
            return Ok(());
        };
        let queries = crate::services::deps::q(state).await?;
        let mut resolved: Vec<Uuid> = Vec::new();
        for raw in user_ids {
            if let Ok(uid) = Uuid::parse_str(&raw) {
                resolved.push(uid);
            }
        }
        for name in usernames {
            if let Ok(user) = queries.get_user_by_username(&name).await {
                resolved.push(user.id);
            }
        }
        for uid in &resolved {
            let _ = session.follows.insert_sync(*uid);
        }
        session.send_message(
            cid.clone(),
            EnvelopeMessage::StatusFollowed {
                user_ids: resolved.iter().map(|u| u.simple().to_string()).collect(),
            },
        );
        let joins = self.online_for(&resolved);
        if !joins.is_empty() {
            session.send(&Envelope {
                cid: None,
                message: EnvelopeMessage::StatusPresenceEvent {
                    joins,
                    leaves: Vec::new(),
                },
            });
        }
        Ok(())
    }
    pub fn unfollow_users(&self, sid: Uuid, user_ids: Vec<String>, cid: Option<String>) {
        let Some(session) = session::find(sid) else {
            return;
        };
        for raw in user_ids {
            if let Ok(uid) = Uuid::parse_str(&raw) {
                let _ = session.follows.remove_sync(&uid);
            }
        }
        session.send_message(
            cid,
            EnvelopeMessage::StatusFollowed {
                user_ids: Vec::new(),
            },
        );
    }
}
