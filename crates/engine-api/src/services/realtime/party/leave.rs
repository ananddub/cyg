use super::*;

impl RealtimePartyService {
    pub async fn leave(&self, conn: &Conn, msg: &Msg) -> Dispatched {
        let req: PartyLeave = msg.payload()?;
        let s = session_of(conn);
        let id =
            Uuid::parse_str(&req.party_id).map_err(|_| V2Error::not_found("party not found"))?;
        let (left, remaining, _closed, _new_leader_sid) = {
            let party_ref = self.parties.get_sync(&id);
            let Some(mut p) = party_ref else {
                s.set_party_id(None);
                return Ok(None);
            };
            let left = p.members.remove(&s.sid);
            let remaining: Vec<Uuid> = p.members.keys().copied().collect();
            let closed = p.members.is_empty();
            let mut leader = p.leader_sid;
            if !closed && leader == s.sid && !remaining.is_empty() {
                leader = remaining[0];
                p.leader_sid = leader;
            }
            drop(p);
            if closed {
                self.parties.remove_sync(&id);
            }
            (left, remaining, closed, leader)
        };
        s.set_party_id(None);
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
        s.send_message(
            msg.cid().map(str::to_string),
            EnvelopeMessage::PartyLeave {
                party_id: id.simple().to_string(),
                metadata: serde_json::Value::Null,
            },
        );
        Ok(None)
    }
    pub async fn promote(&self, conn: &Conn, msg: &Msg) -> Dispatched {
        let req: PartyPromote = msg.payload()?;
        let s = session_of(conn);
        let party_id =
            Uuid::parse_str(&req.party_id).map_err(|_| V2Error::not_found("party not found"))?;
        let target_sid = Uuid::parse_str(&req.presence.session_id)
            .map_err(|_| V2Error::invalid_argument("invalid target session id"))?;
        if s.party_id() != Some(party_id) {
            return Err(V2Error::permission_denied("not a member of this party"));
        }
        let (leader_presence, member_sids) = {
            let mut p = self
                .parties
                .get_sync(&party_id)
                .ok_or_else(|| V2Error::not_found("party not found"))?;
            if p.leader_sid != s.sid {
                return Err(V2Error::permission_denied("only party leader can promote"));
            }
            let presence = p
                .members
                .get(&target_sid)
                .cloned()
                .ok_or_else(|| V2Error::not_found("target is not in party"))?;
            p.leader_sid = target_sid;
            let sids: Vec<Uuid> = p.members.keys().copied().collect();
            (presence, sids)
        };
        let event = Envelope {
            cid: None,
            message: EnvelopeMessage::PartyPromote {
                party_id: party_id.simple().to_string(),
                presence: leader_presence,
            },
        };
        for sid in member_sids {
            if let Some(sess) = session::find(sid) {
                sess.send(&event);
            }
        }
        Ok(None)
    }
    pub async fn update(&self, conn: &Conn, msg: &Msg) -> Dispatched {
        let req: PartyUpdate = msg.payload()?;
        let s = session_of(conn);
        let party_id =
            Uuid::parse_str(&req.party_id).map_err(|_| V2Error::not_found("party not found"))?;
        if s.party_id() != Some(party_id) {
            return Err(V2Error::permission_denied("not a member of this party"));
        }
        {
            let mut p = self
                .parties
                .get_sync(&party_id)
                .ok_or_else(|| V2Error::not_found("party not found"))?;
            if p.leader_sid != s.sid {
                return Err(V2Error::permission_denied(
                    "only party leader can update party",
                ));
            }
            if let Some(open) = req.open {
                p.open = open;
            }
            if let Some(size) = req.max_size {
                p.max_size = size.clamp(p.members.len() as i32, MAX_PARTY_SIZE);
            }
        }
        Ok(None)
    }
}
