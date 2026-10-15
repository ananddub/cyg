//! Online lookups: presence rows and the first live session's presence.

use uuid::Uuid;

use crate::infra::ws::envelope::{Presence, StatusPresence};
use crate::infra::ws::session;

use super::{presences, to_status_presence};

/// Currently-online presence rows for the given user ids.
pub fn online_for(uids: &[Uuid]) -> Vec<StatusPresence> {
    let all = presences().read().expect("presence lock");
    uids.iter()
        .filter_map(|uid| {
            all.get(uid).and_then(|by_sid| {
                by_sid.iter().next().map(|(sid, (username, status))| {
                    to_status_presence(*uid, *sid, username, status)
                })
            })
        })
        .collect()
}

/// Presence snapshot of a user's first live session, for matchmaker and party
/// announcements where only the uid is known.
pub fn presence_of(session: &session::Session) -> Presence {
    let status = presences()
        .read()
        .expect("presence lock")
        .get(&session.uid)
        .and_then(|by_sid| by_sid.get(&session.sid))
        .map(|(_, s)| s.clone())
        .unwrap_or_default();
    let _ = status;
    Presence {
        user_id: session.uid.simple().to_string(),
        session_id: session.sid.simple().to_string(),
        username: session.username.clone(),
        node: None,
    }
}
