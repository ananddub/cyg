//! Socket teardown: announce the departure in every match, prune empties.

use std::sync::Arc;
use uuid::Uuid;

use crate::infra::ws::envelope::{Envelope, EnvelopeMessage};
use crate::infra::ws::session;

use super::matches;

pub fn cleanup(s: &Arc<session::Session>) {
    let ids: Vec<Uuid> = s
        .matches
        .read()
        .expect("match set lock")
        .iter()
        .copied()
        .collect();
    for id in ids {
        let (member_ids, presence) = {
            let mut all = matches().write().expect("match registry lock");
            let Some(m) = all.get_mut(&id) else { continue };
            let presence = m.members.remove(&s.sid);
            let member_ids: Vec<Uuid> = m
                .members
                .keys()
                .filter_map(|sid| session::find(*sid))
                .map(|sess| sess.uid)
                .collect();
            let empty = m.members.is_empty() && m.ephemeral;
            if empty {
                all.remove(&id);
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

/// Whether a matchmaker-created match id still exists in the registry.
pub fn match_exists(id: Uuid) -> bool {
    matches()
        .read()
        .expect("match registry lock")
        .contains_key(&id)
}
