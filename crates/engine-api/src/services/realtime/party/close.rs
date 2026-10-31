use super::*;

impl RealtimePartyService {
    pub async fn close(&self, conn: &Conn, msg: &Msg) -> Dispatched {
        let req: PartyClose = msg.payload()?;
        let s = session_of(conn);
        let party_id =
            Uuid::parse_str(&req.party_id).map_err(|_| V2Error::not_found("party not found"))?;
        if s.party_id() != Some(party_id) {
            return Err(V2Error::permission_denied("not a member of this party"));
        }
        let member_sids = {
            let party_ref = self.parties.get_sync(&party_id);
            let Some(p) = party_ref else { return Ok(None) };
            if p.leader_sid != s.sid {
                return Err(V2Error::permission_denied(
                    "only party leader can close party",
                ));
            }
            let sids: Vec<Uuid> = p.members.keys().copied().collect();
            drop(p);
            self.parties.remove_sync(&party_id);
            sids
        };
        let event = Envelope {
            cid: None,
            message: EnvelopeMessage::PartyClose {
                party_id: party_id.simple().to_string(),
                metadata: req.metadata,
            },
        };
        for sid in member_sids {
            if let Some(sess) = session::find(sid) {
                sess.set_party_id(None);
                sess.send(&event);
            }
        }
        Ok(None)
    }
    pub async fn data_send(&self, conn: &Conn, msg: &Msg) -> Dispatched {
        let req: PartyDataSend = msg.payload()?;
        let s = session_of(conn);
        let party_id =
            Uuid::parse_str(&req.party_id).map_err(|_| V2Error::not_found("party not found"))?;
        if s.party_id() != Some(party_id) {
            return Err(V2Error::permission_denied("not a member of this party"));
        }
        let presence = self.presence_of(&s);
        let event = Envelope {
            cid: None,
            message: EnvelopeMessage::PartyData {
                party_id: req.party_id.clone(),
                presence: Some(presence),
                op_code: req.op_code,
                data: req.data,
            },
        };
        self.parties.read_sync(&party_id, |_, p| {
            for sid in p.members.keys() {
                if *sid != s.sid {
                    if let Some(sess) = session::find(*sid) {
                        sess.send(&event);
                    }
                }
            }
        });
        Ok(None)
    }
    pub async fn matchmaker_add(&self, conn: &Conn, msg: &Msg) -> Dispatched {
        let req: PartyMatchmakerAdd = msg.payload()?;
        let s = session_of(conn);
        let Some(party_id) = s.party_id() else {
            s.send_message(
                msg.cid().map(str::to_string),
                EnvelopeMessage::Error(ErrorMsg {
                    message: "not a member of a party".into(),
                }),
            );
            return Ok(None);
        };
        let ticket = match super::super::RealtimeMatchmakerService::add_ticket(
            &s,
            req.min_count,
            req.max_count,
            req.query,
            req.string_properties,
            req.numeric_properties,
        ) {
            Ok(t) => t,
            Err(err_msg) => {
                s.send_message(msg.cid().map(str::to_string), err_msg);
                return Ok(None);
            }
        };
        s.send_message(
            msg.cid().map(str::to_string),
            EnvelopeMessage::PartyMatchmakerTicket {
                party_id: party_id.simple().to_string(),
                ticket,
            },
        );
        Ok(None)
    }
    pub async fn matchmaker_remove(&self, conn: &Conn, msg: &Msg) -> Dispatched {
        let req: PartyMatchmakerRemove = msg.payload()?;
        let s = session_of(conn);
        let party_id =
            Uuid::parse_str(&req.party_id).map_err(|_| V2Error::not_found("party not found"))?;
        if s.party_id() != Some(party_id) {
            return Err(V2Error::permission_denied("not a member of this party"));
        }
        if super::super::RealtimeMatchmakerService::remove_ticket(&s, &req.ticket) {
            s.send_message(
                msg.cid().map(str::to_string),
                EnvelopeMessage::MatchmakerDone,
            );
        }
        Ok(None)
    }
}
