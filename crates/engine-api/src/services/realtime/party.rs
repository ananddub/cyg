//! Realtime Party Service (Squads, Admin, and Matchmaker Integration) in a single consolidated file.
//!
//! All state and methods are fully encapsulated on the `RealtimePartyService` struct as lock-free `FastMap`s,
//! injected as a singleton via `auto_di` with zero free functions, statics, or mutexes.

use std::collections::HashMap;
use std::sync::Arc;
use auto_di::singleton;
use auto_ws::{Conn, Msg};
use uuid::Uuid;

use crate::error::V2Error;
use crate::infra::ws::envelope::{Envelope, EnvelopeMessage, ErrorMsg, Party, Presence};
use crate::infra::ws::session::{self, Session};
use crate::controllers::realtime::messages::*;
use crate::controllers::realtime::{Dispatched, session_of};
use super::{FastMap, new_fast_map};

const MAX_PARTY_SIZE: i32 = 4;

pub struct PartyState {
    pub id: Uuid,
    pub leader_sid: Uuid,
    pub open: bool,
    pub max_size: i32,
    pub members: HashMap<Uuid /*sid*/, Presence>,
}

pub struct RealtimePartyService {
    pub parties: FastMap<Uuid, PartyState>,
}

#[singleton]
impl RealtimePartyService {
    pub fn new() -> Self {
        Self {
            parties: new_fast_map(),
        }
    }
}

impl RealtimePartyService {
    // --- Private Helper Methods ---

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

    fn presence_of(&self, s: &Session) -> Presence {
        Presence {
            user_id: s.uid.simple().to_string(),
            session_id: s.sid.simple().to_string(),
            username: s.username.clone(),
            node: None,
        }
    }

    // --- Lifecycle Management ---

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

    // --- Realtime WebSocket Event Handlers ---

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
        s.send_message(msg.cid().map(str::to_string), EnvelopeMessage::PartyReply { party: reply });
        Ok(None)
    }

    pub async fn join(&self, conn: &Conn, msg: &Msg) -> Dispatched {
        let req: PartyJoin = msg.payload()?;
        let s = session_of(conn);
        let id = Uuid::parse_str(&req.party_id).map_err(|_| V2Error::not_found("party not found"))?;
        let (joined, member_sids) = {
            let mut p = self.parties
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
        s.send_message(msg.cid().map(str::to_string), EnvelopeMessage::PartyReply { party: joined });
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

    pub async fn leave(&self, conn: &Conn, msg: &Msg) -> Dispatched {
        let req: PartyLeave = msg.payload()?;
        let s = session_of(conn);
        let id = Uuid::parse_str(&req.party_id).map_err(|_| V2Error::not_found("party not found"))?;
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
        let party_id = Uuid::parse_str(&req.party_id).map_err(|_| V2Error::not_found("party not found"))?;
        let target_sid = Uuid::parse_str(&req.presence.session_id).map_err(|_| V2Error::invalid_argument("invalid target session id"))?;
        if s.party_id() != Some(party_id) {
            return Err(V2Error::permission_denied("not a member of this party"));
        }
        let (leader_presence, member_sids) = {
            let mut p = self.parties.get_sync(&party_id).ok_or_else(|| V2Error::not_found("party not found"))?;
            if p.leader_sid != s.sid {
                return Err(V2Error::permission_denied("only party leader can promote"));
            }
            let presence = p.members.get(&target_sid).cloned().ok_or_else(|| V2Error::not_found("target is not in party"))?;
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
        let party_id = Uuid::parse_str(&req.party_id).map_err(|_| V2Error::not_found("party not found"))?;
        if s.party_id() != Some(party_id) {
            return Err(V2Error::permission_denied("not a member of this party"));
        }
        {
            let mut p = self.parties.get_sync(&party_id).ok_or_else(|| V2Error::not_found("party not found"))?;
            if p.leader_sid != s.sid {
                return Err(V2Error::permission_denied("only party leader can update party"));
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

    pub async fn close(&self, conn: &Conn, msg: &Msg) -> Dispatched {
        let req: PartyClose = msg.payload()?;
        let s = session_of(conn);
        let party_id = Uuid::parse_str(&req.party_id).map_err(|_| V2Error::not_found("party not found"))?;
        if s.party_id() != Some(party_id) {
            return Err(V2Error::permission_denied("not a member of this party"));
        }
        let member_sids = {
            let party_ref = self.parties.get_sync(&party_id);
            let Some(p) = party_ref else { return Ok(None) };
            if p.leader_sid != s.sid {
                return Err(V2Error::permission_denied("only party leader can close party"));
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
        let party_id = Uuid::parse_str(&req.party_id).map_err(|_| V2Error::not_found("party not found"))?;
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
        let ticket = match super::RealtimeMatchmakerService::add_ticket(&s, req.min_count, req.max_count, req.query, req.string_properties, req.numeric_properties) {
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
        let party_id = Uuid::parse_str(&req.party_id).map_err(|_| V2Error::not_found("party not found"))?;
        if s.party_id() != Some(party_id) {
            return Err(V2Error::permission_denied("not a member of this party"));
        }
        if super::RealtimeMatchmakerService::remove_ticket(&s, &req.ticket) {
            s.send_message(msg.cid().map(str::to_string), EnvelopeMessage::MatchmakerDone);
        }
        Ok(None)
    }
}
