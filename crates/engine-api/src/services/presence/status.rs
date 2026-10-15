//! `status_update`: store the caller's status and announce it to followers.

use uuid::Uuid;

use crate::ApiState;
use crate::infra::ws::envelope::{Envelope, EnvelopeMessage};
use crate::infra::ws::session;

use super::{followers_of, presences, to_status_presence};

pub fn update_status(state: &ApiState, sid: Uuid, status: String, cid: Option<String>) {
    let _ = state;
    let Some(session) = session::find(sid) else {
        return;
    };
    {
        let mut all = presences().write().expect("presence lock");
        let by_sid = all.entry(session.uid).or_default();
        by_sid.insert(session.sid, (session.username.clone(), status.clone()));
    }
    // BaaS shows one presence per user, taking the first session's status;
    // followers get a join. The caller's other sessions also see the refresh.
    let followers = followers_of(session.uid);
    let presence = to_status_presence(session.uid, session.sid, &session.username, &status);
    for f in followers {
        if f.sid == sid {
            continue;
        }
        let message = EnvelopeMessage::StatusPresenceEvent {
            joins: vec![presence.clone()],
            leaves: Vec::new(),
        };
        f.send(&Envelope { cid: cid.clone(), message });
    }
    for own in session::sessions_of(session.uid) {
        if own.sid != sid {
            own.send_message(
                cid.clone(),
                EnvelopeMessage::StatusPresenceEvent {
                    joins: vec![presence.clone()],
                    leaves: Vec::new(),
                },
            );
        }
    }
}
