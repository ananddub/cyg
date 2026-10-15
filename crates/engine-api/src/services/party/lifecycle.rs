//! Party create/join/leave.

use std::collections::HashMap;
use std::sync::Arc;
use uuid::Uuid;

use crate::error::V2Error;
use crate::infra::ws::envelope::{Envelope, EnvelopeMessage};
use crate::infra::ws::session;
use crate::services::presence;

use super::{parties, wire_of, PartyState, MAX_PARTY_SIZE};

pub fn create(
    s: &Arc<session::Session>,
    max_size: i32,
    open: bool,
    cid: Option<String>,
) -> Result<(), V2Error> {
    let size = max_size.clamp(2, MAX_PARTY_SIZE);
    let id = Uuid::new_v4();
    let presence = presence::presence_of(s);
    let p = PartyState {
        id,
        leader_sid: s.sid,
        open,
        max_size: size,
        members: HashMap::from([(s.sid, presence)]),
    };
    let reply = wire_of(&p, s.sid);
    parties()
        .write()
        .expect("party registry lock")
        .insert(id, p);
    *s.party.write().expect("party lock") = Some(id);
    s.send_message(cid, EnvelopeMessage::PartyReply { party: reply });
    Ok(())
}

pub fn join(s: &Arc<session::Session>, raw_id: &str, cid: Option<String>) -> Result<(), V2Error> {
    let id = Uuid::parse_str(raw_id).map_err(|_| V2Error::not_found("party not found"))?;
    let (joined, member_sids) = {
        let mut all = parties().write().expect("party registry lock");
        let p = all
            .get_mut(&id)
            .ok_or_else(|| V2Error::not_found("party not found"))?;
        if !p.open {
            return Err(V2Error::permission_denied("party is private"));
        }
        if p.members.len() as i32 >= p.max_size {
            return Err(V2Error::failed_precondition("party is full"));
        }
        let presence = presence::presence_of(s);
        p.members.insert(s.sid, presence);
        let wire = wire_of(p, s.sid);
        let sids: Vec<Uuid> = p.members.keys().copied().collect();
        (wire, sids)
    };
    *s.party.write().expect("party lock") = Some(id);
    s.send_message(cid, EnvelopeMessage::PartyReply { party: joined });
    let event = Envelope {
        cid: None,
        message: EnvelopeMessage::PartyMemberJoin {
            party_id: id.simple().to_string(),
            joins: vec![presence::presence_of(s)],
        },
    };
    for sid in member_sids {
        if sid != s.sid {
            if let Some(sess) = session::find(sid) {
                sess.send(&event);
            }
        }
    }
    Ok(())
}

/// Leader exit hands over to the oldest remaining member; the last member
/// leaving closes the party.
pub fn leave(s: &Arc<session::Session>, raw_id: &str, cid: Option<String>) -> Result<(), V2Error> {
    let id = Uuid::parse_str(raw_id).map_err(|_| V2Error::not_found("party not found"))?;
    let (left, remaining, closed, new_leader_sid) = {
        let mut all = parties().write().expect("party registry lock");
        let Some(p) = all.get_mut(&id) else {
            *s.party.write().expect("party lock") = None;
            return Ok(());
        };
        let left = p.members.remove(&s.sid);
        let remaining: Vec<Uuid> = p.members.keys().copied().collect();
        let closed = p.members.is_empty();
        let mut leader = p.leader_sid;
        if !closed {
            if leader == s.sid {
                leader = remaining[0];
                p.leader_sid = leader;
            }
        }
        let result = (left, remaining, closed, leader);
        if closed {
            all.remove(&id);
        }
        result
    };
    *s.party.write().expect("party lock") = None;
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
        if closed {
            let close = Envelope {
                cid: None,
                message: EnvelopeMessage::PartyClose {
                    party_id: id.simple().to_string(),
                    metadata: serde_json::Value::Null,
                },
            };
            for sid in remaining {
                if let Some(sess) = session::find(sid) {
                    sess.send(&close);
                }
            }
        } else if new_leader_sid != Uuid::nil() {
            let _ = new_leader_sid;
        }
    }
    s.send_message(
        cid,
        EnvelopeMessage::PartyLeave {
            party_id: raw_id.to_string(),
            metadata: serde_json::Value::Null,
        },
    );
    Ok(())
}
