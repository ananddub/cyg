//! The console's view of running matches: the list rows, and a single match's
//! live state.
//!
//! These mirror the reference server's `console.MatchList` / `console.MatchState`. The list wraps
//! each entry's `api_match` (the same shape the player route answers) in a console
//! row and adds the `node` it runs on; this plane is a single process, so the node
//! is empty rather than invented.

use serde::Serialize;

use crate::dto::matches::MatchResponse;
use crate::infra::ws::envelope::Presence;
use crate::services::realtime::matches::MatchSnapshot;

/// `console.MatchList.Match`: an `api_match` plus its node.
#[derive(Debug, Serialize)]
pub struct ConsoleMatchRow {
    pub api_match: MatchResponse,
    pub node: String,
}

impl From<&MatchSnapshot> for ConsoleMatchRow {
    fn from(m: &MatchSnapshot) -> Self {
        Self {
            api_match: MatchResponse::from(m),
            node: String::new(),
        }
    }
}

/// `console.MatchList`.
#[derive(Debug, Serialize)]
pub struct ConsoleMatchList {
    pub matches: Vec<ConsoleMatchRow>,
}

/// `console.MatchState`: the members currently in the match, its tick and its
/// handler state. A relay match holds no handler state and tracks no tick counter,
/// so `tick` is `0` and `state` the empty string — the same answer reference gives a
/// relayed match. The `presences` are the real joined members, which is the part an
/// operator can act on.
#[derive(Debug, Serialize)]
pub struct ConsoleMatchState {
    pub presences: Vec<Presence>,
    pub tick: i64,
    pub state: String,
}
