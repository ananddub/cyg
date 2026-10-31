use super::*;

impl RealtimePartyService {
    fn wire_of(&self, p: &PartyState, self_sid: Uuid) -> Party {
        let me = p.members.get(&self_sid).cloned().unwrap_or_default();
        Party {
            party_id: p.id.simple().to_string(),
            open: p.open,
            max_size: p.max_size,
            self_presence: me,
            leader: p.members.get(&p.leader_sid).cloned().unwrap_or_default(),
            presences: p
                .members
                .values()
                .filter(|x| x.session_id != self_sid.simple().to_string())
                .cloned()
                .collect(),
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
    pub fn cleanup(&self, s: &Arc<Session>) {
        let Some(id) = s.party_id() else {
            return;
        };
        let (left, remaining, _closed) = {
            let party_ref = self.parties.get_sync(&id);
            let Some(mut p) = party_ref else { return };
            let left = p.members.remove(&s.sid);
            let remaining: Vec<Uuid> = p.members.keys().copied().collect();
            let closed = p.members.is_empty();
            if !closed && p.leader_sid == s.sid && !remaining.is_empty() {
                p.leader_sid = remaining[0];
            }
            drop(p);
            if closed {
                self.parties.remove_sync(&id);
            }
            (left, remaining, closed)
        };
        if let Some(presence) = left {
            let event = Envelope {
                cid: None,
                message: EnvelopeMessage::PartyMemberLeave {
                    party_id: id.simple().to_string(),
                    leaves: vec![presence],
                },
            };
            for sid in &remaining {
                if let Some(sess) = session::find(*sid) {
                    sess.send(&event);
                }
            }
        }
    }
    pub async fn create(&self, conn: &Conn, msg: &Msg) -> Dispatched {
        let req: PartyCreate = msg.payload()?;
        let s = session_of(conn);
        let size = req.max_size.clamp(2, MAX_PARTY_SIZE);
        let id = Uuid::new_v4();
        let presence = self.presence_of(&s);
        let p = PartyState {
            id,
            leader_sid: s.sid,
            open: req.open,
            max_size: size,
            members: HashMap::from([(s.sid, presence)]),
        };
        let reply = self.wire_of(&p, s.sid);
        let _ = self.parties.insert_sync(id, p);
        s.set_party_id(Some(id));
        s.send_message(
            msg.cid().map(str::to_string),
            EnvelopeMessage::PartyReply { party: reply },
        );
        Ok(None)
    }
    pub async fn join(&self, conn: &Conn, msg: &Msg) -> Dispatched {
        let req: PartyJoin = msg.payload()?;
        let s = session_of(conn);
        let id =
            Uuid::parse_str(&req.party_id).map_err(|_| V2Error::not_found("party not found"))?;
        let (joined, member_sids) = {
            let mut p = self
                .parties
                .get_sync(&id)
                .ok_or_else(|| V2Error::not_found("party not found"))?;
            if !p.open {
                return Err(V2Error::permission_denied("party is private"));
            }
            if p.members.len() as i32 >= p.max_size {
                return Err(V2Error::failed_precondition("party is full"));
            }
            let presence = self.presence_of(&s);
            p.members.insert(s.sid, presence);
            let wire = self.wire_of(&p, s.sid);
            let sids: Vec<Uuid> = p.members.keys().copied().collect();
            (wire, sids)
        };
        s.set_party_id(Some(id));
        s.send_message(
            msg.cid().map(str::to_string),
            EnvelopeMessage::PartyReply { party: joined },
        );
        let event = Envelope {
            cid: None,
            message: EnvelopeMessage::PartyMemberJoin {
                party_id: id.simple().to_string(),
                joins: vec![self.presence_of(&s)],
            },
        };
        for sid in member_sids {
            if sid != s.sid {
                if let Some(sess) = session::find(sid) {
                    sess.send(&event);
                }
            }
        }
        Ok(None)
    }
}
