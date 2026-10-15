//! `match_create` / `match_join` / `match_leave` and matchmaker minting.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use uuid::Uuid;

use crate::error::V2Error;
use crate::infra::ws::envelope::{Envelope, EnvelopeMessage, Presence};
use crate::infra::ws::session;

use super::{matches, notify, presence_of, Match};

/// `match_create`: mint an id, keep it closed unless `open`.
pub fn create(s: &Arc<session::Session>, open: bool, tick_rate: i32, label: String) -> Uuid {
    let id = Uuid::new_v4();
    let mut all = matches().write().expect("match registry lock");
    all.insert(
        id,
        Match {
            id,
            label,
            open,
            tick_rate,
            members: HashMap::new(),
            tokens: HashSet::new(),
            ephemeral: false,
        },
    );
    let _ = s;
    id
}

/// Internal creation for the matchmaker: closed, token-gated.
pub fn create_with_tokens(tick_rate: i32, tokens: Vec<String>) -> Uuid {
    let id = Uuid::new_v4();
    let mut all = matches().write().expect("match registry lock");
    all.insert(
        id,
        Match {
            id,
            label: String::new(),
            open: false,
            tick_rate,
            members: HashMap::new(),
            tokens: tokens.into_iter().collect(),
            ephemeral: true,
        },
    );
    id
}

/// Resolve `match_join` arguments to a match id: explicit id, token, or an
/// open match carrying the label (created on demand when none exists).
pub fn join(
    s: &Arc<session::Session>,
    match_id: Option<String>,
    token: Option<String>,
    label: Option<String>,
    cid: Option<String>,
) -> Result<(), V2Error> {
    let sid = s.sid;
    let mut all = matches().write().expect("match registry lock");

    let target: Uuid = if let Some(raw) = match_id.filter(|m| !m.is_empty()) {
        let id = Uuid::parse_str(&raw).map_err(|_| V2Error::not_found("match not found"))?;
        if !all.contains_key(&id) {
            return Err(V2Error::not_found("match not found"));
        }
        id
    } else if let Some(tok) = token.as_ref().filter(|t| !t.is_empty()) {
        let found = all
            .iter()
            .find(|(_, m)| m.tokens.contains(tok))
            .map(|(id, _)| *id);
        found.ok_or_else(|| V2Error::not_found("match token invalid or expired"))?
    } else if let Some(lbl) = label.filter(|l| !l.is_empty()) {
        match all.values_mut().find(|m| m.label == lbl && m.open) {
            Some(m) => m.id,
            None => {
                let id = Uuid::new_v4();
                all.insert(
                    id,
                    Match {
                        id,
                        label: lbl,
                        open: true,
                        tick_rate: 0,
                        members: HashMap::new(),
                        tokens: HashSet::new(),
                        ephemeral: false,
                    },
                );
                id
            }
        }
    } else {
        return Err(V2Error::invalid_argument(
            "match_join needs match_id, token or label",
        ));
    };

    let match_ref = all.get_mut(&target).expect("target exists");
    if let Some(tok) = token.as_ref().filter(|t| !t.is_empty()) {
        match_ref.tokens.remove(tok);
    } else if !match_ref.open || match_ref.ephemeral {
        return Err(V2Error::permission_denied("not permitted to join match"));
    }
    let mine = presence_of(s);
    let others: Vec<Presence> = match_ref.members.values().cloned().collect();
    match_ref.members.insert(sid, mine.clone());
    let member_ids: Vec<Uuid> = match_ref.members.keys().copied().collect();
    drop(all);

    s.matches.write().expect("match set lock").insert(target);
    // the reply: the joiner sees the room, everyone else sees a join.
    s.send_message(
        cid.clone(),
        EnvelopeMessage::MatchJoin {
            match_id: Some(target.simple().to_string()),
            token: None,
            label: None,
            presences: others,
        },
    );
    let event = Envelope {
        cid: None,
        message: EnvelopeMessage::MatchPresenceEvent {
            match_id: target.simple().to_string(),
            joins: vec![mine],
            leaves: Vec::new(),
        },
    };
    notify(&member_ids, sid, &event);
    Ok(())
}

pub fn leave(s: &Arc<session::Session>, raw_id: &str, cid: Option<String>) -> Result<(), V2Error> {
    let id = Uuid::parse_str(raw_id).map_err(|_| V2Error::not_found("match not found"))?;
    let (member_ids, gone) = {
        let mut all = matches().write().expect("match registry lock");
        let Some(m) = all.get_mut(&id) else {
            s.matches.write().expect("match set lock").remove(&id);
            return Ok(());
        };
        let gone = m.members.remove(&s.sid);
        let member_ids: Vec<Uuid> = m.members.keys().copied().collect();
        let empty = m.members.is_empty() && (m.ephemeral || m.tokens.is_empty());
        if empty {
            all.remove(&id);
        }
        (member_ids, gone)
    };
    s.matches.write().expect("match set lock").remove(&id);
    if let Some(presence) = gone {
        let event = Envelope {
            cid: None,
            message: EnvelopeMessage::MatchPresenceEvent {
                match_id: id.simple().to_string(),
                joins: Vec::new(),
                leaves: vec![presence],
            },
        };
        notify(&member_ids, s.sid, &event);
    }
    s.send_message(
        cid,
        EnvelopeMessage::MatchLeave {
            match_id: raw_id.to_string(),
        },
    );
    Ok(())
}
