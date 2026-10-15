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
pub use lifecycle::{create, create_with_tokens, join, leave};

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
