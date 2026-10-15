//! Socket teardown: announce the departure exactly like `leave` would.

use std::sync::Arc;
use uuid::Uuid;

use crate::infra::ws::envelope::{Envelope, EnvelopeMessage};
use crate::infra::ws::session;
use crate::services::match_handler;

use super::parties;

pub fn cleanup(s: &Arc<session::Session>) {
    let Some(id) = *s.party.read().expect("party lock") else {
        return;
    };
    let (left, remaining, closed) = {
        let mut all = parties().write().expect("party registry lock");
        let Some(p) = all.get_mut(&id) else { return };
        let left = p.members.remove(&s.sid);
        let remaining: Vec<Uuid> = p.members.keys().copied().collect();
        let closed = p.members.is_empty();
        if closed {
            all.remove(&id);
        } else if p.leader_sid == s.sid {
            p.leader_sid = remaining[0];
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
        if closed {
            let _ = match_handler::match_exists(id);
        }
    }
}
