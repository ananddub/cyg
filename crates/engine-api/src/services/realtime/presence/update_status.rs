use super::*;

impl RealtimePresenceService {
    pub fn update_status(&self, sid: Uuid, status: String) {
        let Some(session) = session::find(sid) else {
            return;
        };
        {
            let mut entry = self.presences.entry_sync(session.uid).or_default();
            entry
                .get_mut()
                .insert(session.sid, (session.username.clone(), status.clone()));
        }
        let followers = self.followers_of(session.uid);
        let presence =
            self.to_status_presence(session.uid, session.sid, &session.username, &status);
        for f in followers {
            if f.sid == sid {
                continue;
            }
            let message = EnvelopeMessage::StatusPresenceEvent {
                joins: vec![presence.clone()],
                leaves: Vec::new(),
            };
            f.send(&Envelope { cid: None, message });
        }
        for own in session::sessions_of(session.uid) {
            if own.sid != sid {
                own.send_message(
                    None,
                    EnvelopeMessage::StatusPresenceEvent {
                        joins: vec![presence.clone()],
                        leaves: Vec::new(),
                    },
                );
            }
        }
    }
    pub async fn follow(&self, conn: &Conn, msg: &Msg) -> Dispatched {
        let req: StatusFollow = msg.payload()?;
        self.follow_users(
            &state_of(conn),
            conn.id(),
            req.user_ids,
            req.usernames,
            msg.cid().map(str::to_string),
        )
        .await
        .map(|_| None)
    }
    pub async fn unfollow(&self, conn: &Conn, msg: &Msg) -> Dispatched {
        let req: StatusUnfollow = msg.payload()?;
        self.unfollow_users(conn.id(), req.user_ids, msg.cid().map(str::to_string));
        Ok(None)
    }
    pub async fn update(&self, conn: &Conn, msg: &Msg) -> Dispatched {
        let req: StatusUpdate = msg.payload()?;
        self.update_status(conn.id(), req.status);
        Ok(None)
    }
}
