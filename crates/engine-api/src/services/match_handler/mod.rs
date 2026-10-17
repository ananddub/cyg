//! Multiplayer matches (relay); BaaS `pipeline_match.go` parity.

use std::collections::{HashMap, HashSet};
use std::sync::{OnceLock, RwLock};
use uuid::Uuid;

use crate::infra::ws::envelope::{Envelope, Presence};
use crate::infra::ws::session;
use crate::services::presence;

pub mod cleanup;
pub mod data;
pub mod lifecycle;

pub use cleanup::{cleanup, match_exists};
pub use data::{data_send, signal};
pub use lifecycle::{create, create_with_tokens, join, leave, terminate};

struct Match {
    id: Uuid,
    label: String,
    open: bool,
    tick_rate: i32,
    members: HashMap<Uuid /*sid*/, Presence>,
    /// Matchmaker tokens: each permits one join.
    tokens: HashSet<String>,
    /// True when a matchmaker created this match: it only ever admits
    /// token holders, never a direct second joiner.
    ephemeral: bool,
}

static MATCHES: OnceLock<RwLock<HashMap<Uuid, Match>>> = OnceLock::new();

fn matches() -> &'static RwLock<HashMap<Uuid, Match>> {
    MATCHES.get_or_init(|| RwLock::new(HashMap::new()))
}

/// A live match as the read side sees it: the registry entry minus its join
/// tokens, which a console operator has no business reading.
pub struct MatchSnapshot {
    pub id: Uuid,
    pub label: String,
    pub open: bool,
    pub tick_rate: i32,
    pub members: Vec<Presence>,
}

/// Every running match. The in-process registry is a match's only home, so this
/// — not any table — is the real source the introspection routes read.
pub fn list_snapshots() -> Vec<MatchSnapshot> {
    matches()
        .read()
        .expect("match registry lock")
        .values()
        .map(snapshot_of)
        .collect()
}

/// One running match by id, or `None` when it has already stopped.
pub fn snapshot(id: Uuid) -> Option<MatchSnapshot> {
    matches()
        .read()
        .expect("match registry lock")
        .get(&id)
        .map(snapshot_of)
}

fn snapshot_of(m: &Match) -> MatchSnapshot {
    MatchSnapshot {
        id: m.id,
        label: m.label.clone(),
        open: m.open,
        tick_rate: m.tick_rate,
        members: m.members.values().cloned().collect(),
    }
}

fn presence_of(s: &session::Session) -> Presence {
    presence::presence_of(s)
}

/// Deliver to all live sessions of the given users except `except_sid`.
fn notify(user_ids: &[Uuid], except_sid: Uuid, event: &Envelope) {
    let mut seen = HashSet::new();
    for uid in user_ids {
        if !seen.insert(*uid) {
            continue;
        }
        for sess in session::sessions_of(*uid) {
            if sess.sid != except_sid {
                sess.send(event);
            }
        }
    }
}
