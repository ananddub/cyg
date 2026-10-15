//! `status_follow` / `status_unfollow`.

use uuid::Uuid;

use crate::ApiState;
use crate::error::V2Error;
use crate::infra::ws::envelope::{Envelope, EnvelopeMessage};
use crate::infra::ws::session;

use super::online::online_for;

/// Resolve ids/usernames, remember the edges, reply with the ids actually
/// followed and the online subset as a presence event.
pub async fn follow(
    state: &ApiState,
    sid: Uuid,
    user_ids: Vec<String>,
    usernames: Vec<String>,
    cid: Option<String>,
) -> Result<(), V2Error> {
    let Some(session) = session::find(sid) else {
        return Ok(());
    };
    let queries = crate::services::auth::q(state)?;
    let mut resolved: Vec<Uuid> = Vec::new();
    for raw in user_ids {
        if let Ok(uid) = Uuid::parse_str(&raw) {
            resolved.push(uid);
        }
    }
    for name in usernames {
        if let Ok(user) = queries.get_user_by_username(&name).await {
            resolved.push(user.id);
        }
    }
    {
        let mut follows = session.follows.write().expect("follows lock");
        for uid in &resolved {
            follows.insert(*uid);
        }
    }
    session.send_message(
        cid.clone(),
        EnvelopeMessage::StatusFollowed {
            user_ids: resolved.iter().map(|u| u.simple().to_string()).collect(),
        },
    );
    let joins = online_for(&resolved);
    if !joins.is_empty() {
        session.send(&Envelope {
            cid: None,
            message: EnvelopeMessage::StatusPresenceEvent {
                joins,
                leaves: Vec::new(),
            },
        });
    }
    Ok(())
}

/// `status_unfollow`: drop edges only.
pub fn unfollow(sid: Uuid, user_ids: Vec<String>, cid: Option<String>) {
    let Some(session) = session::find(sid) else {
        return;
    };
    {
        let mut follows = session.follows.write().expect("follows lock");
        for raw in user_ids {
            if let Ok(uid) = Uuid::parse_str(&raw) {
                follows.remove(&uid);
            }
        }
    }
    session.send_message(
        cid,
        EnvelopeMessage::StatusFollowed { user_ids: Vec::new() },
    );
}
