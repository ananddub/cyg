//! `on_disconnect`: tell followers the user went offline.

use uuid::Uuid;

use crate::infra::ws::envelope::{Envelope, EnvelopeMessage};
use crate::infra::ws::session;

use super::{followers_of, presences, to_status_presence};

pub fn on_disconnect(sid: Uuid) {
    let Some(session) = session::find(sid) else {
        return;
    };
    let gone = {
        let mut all = presences().write().expect("presence lock");
        let Some(by_sid) = all.get_mut(&session.uid) else {
            return;
        };
        by_sid.remove(&sid);
        if by_sid.is_empty() {
            all.remove(&session.uid);
            true
        } else {
            false
        }
    };
    if gone {
        // The user has no session left: followers get a leave.
        let leaves = vec![to_status_presence(session.uid, session.sid, &session.username, "")];
        for f in followers_of(session.uid) {
            f.send(&Envelope {
                cid: None,
                message: EnvelopeMessage::StatusPresenceEvent {
                    joins: Vec::new(),
                    leaves: leaves.clone(),
                },
            });
        }
    }
}
