//! Leader-only party admin: promote, update capacity/visibility, close.

use std::sync::Arc;
use uuid::Uuid;

use crate::error::V2Error;
use crate::infra::ws::envelope::{Envelope, EnvelopeMessage, Presence};
use crate::infra::ws::session;

use super::{parties, wire_of, MAX_PARTY_SIZE};

pub fn promote(
    s: &Arc<session::Session>,
    raw_id: &str,
    target: Presence,
    cid: Option<String>,
) -> Result<(), V2Error> {
    let id = Uuid::parse_str(raw_id).map_err(|_| V2Error::not_found("party not found"))?;
    let target_sid = Uuid::parse_str(&target.session_id)
        .map_err(|_| V2Error::invalid_argument("invalid presence"))?;
    let (member_sids, leader_uid) = {
        let mut all = parties().write().expect("party registry lock");
        let p = all
            .get_mut(&id)
            .ok_or_else(|| V2Error::not_found("party not found"))?;
        if p.leader_sid != s.sid {
            return Err(V2Error::permission_denied(
                "only the party leader can promote",
            ));
        }
        if !p.members.contains_key(&target_sid) {
            return Err(V2Error::not_found("presence is not in this party"));
        }
        p.leader_sid = target_sid;
        let sids: Vec<Uuid> = p.members.keys().copied().collect();
        let leader_uid = p.members[&target_sid].user_id.clone();
        (sids, leader_uid)
    };
    // BaaS announces a new leader by resending the full `party` update to
    // every member rather than a dedicated event.
    let guard = parties().read().expect("party registry lock");
    let p = guard.get(&id).expect("exists");
    for sid in member_sids {
        if let Some(sess) = session::find(sid) {
            sess.send(&Envelope {
                cid: if sess.sid == s.sid { cid.clone() } else { None },
                message: EnvelopeMessage::PartyReply {
                    party: wire_of(p, sess.sid),
                },
            });
        }
    }
    let _ = leader_uid;
    Ok(())
}

pub fn update(
    s: &Arc<session::Session>,
    raw_id: &str,
    max_size: Option<i32>,
    open: Option<bool>,
    cid: Option<String>,
) -> Result<(), V2Error> {
    let id = Uuid::parse_str(raw_id).map_err(|_| V2Error::not_found("party not found"))?;
    let guard_ids = {
        let mut all = parties().write().expect("party registry lock");
        let p = all
            .get_mut(&id)
            .ok_or_else(|| V2Error::not_found("party not found"))?;
        if p.leader_sid != s.sid {
            return Err(V2Error::permission_denied(
                "only the party leader can update the party",
            ));
        }
        if let Some(m) = max_size {
            if (m as usize) < p.members.len() {
                return Err(V2Error::invalid_argument(
                    "max_size cannot be below the current member count",
                ));
            }
            p.max_size = m.clamp(2, MAX_PARTY_SIZE);
        }
        if let Some(o) = open {
            p.open = o;
        }
        p.members.keys().copied().collect::<Vec<_>>()
    };
    let guard = parties().read().expect("party registry lock");
    let p = guard.get(&id).expect("exists");
    for sid in guard_ids {
        if let Some(sess) = session::find(sid) {
            sess.send(&Envelope {
                cid: if sess.sid == s.sid { cid.clone() } else { None },
                message: EnvelopeMessage::PartyReply {
                    party: wire_of(p, sess.sid),
                },
            });
        }
    }
    Ok(())
}

pub fn close(
    s: &Arc<session::Session>,
    raw_id: &str,
    metadata: serde_json::Value,
    cid: Option<String>,
) -> Result<(), V2Error> {
    let id = Uuid::parse_str(raw_id).map_err(|_| V2Error::not_found("party not found"))?;
    let member_sids = {
        let mut all = parties().write().expect("party registry lock");
        let Some(p) = all.get_mut(&id) else {
            return Ok(());
        };
        if p.leader_sid != s.sid {
            return Err(V2Error::permission_denied(
                "only the party leader can close the party",
            ));
        }
        let sids: Vec<Uuid> = p.members.keys().copied().collect();
        all.remove(&id);
        sids
    };
    for sid in member_sids {
        if let Some(sess) = session::find(sid) {
            *sess.party.write().expect("party lock") = None;
            sess.send(&Envelope {
                cid: if sess.sid == s.sid { cid.clone() } else { None },
                message: EnvelopeMessage::PartyClose {
                    party_id: id.simple().to_string(),
                    // The closer's own metadata is not echoed back to them.
                    metadata: if sess.sid == s.sid {
                        serde_json::Value::Null
                    } else {
                        metadata.clone()
                    },
                },
            });
        }
    }
    Ok(())
}
