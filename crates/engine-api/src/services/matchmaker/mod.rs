//! Matchmaker: BaaS `server/matchmaker.go` parity in miniature.
//! Tickets pool in memory; a 5-second sweep groups compatible ones into a relay match.

use std::collections::HashMap;
use std::sync::{OnceLock, RwLock};
use uuid::Uuid;

mod matcher;
mod query;
mod sweep;
mod ticket;

#[cfg(test)]
mod tests;

pub use matcher::try_match;
pub use query::query_matches;
pub use ticket::{add, drop_tickets, remove};

#[derive(Clone)]
struct Ticket {
    id: Uuid,
    sid: Uuid,
    uid: Uuid,
    username: String,
    min_count: i32,
    max_count: i32,
    query: String,
    string_props: HashMap<String, String>,
    numeric_props: HashMap<String, f64>,
    party_id: Option<Uuid>,
}

static POOL: OnceLock<RwLock<Vec<Ticket>>> = OnceLock::new();
static TICKER_ONCE: OnceLock<()> = OnceLock::new();

fn pool() -> &'static RwLock<Vec<Ticket>> {
    POOL.get_or_init(|| RwLock::new(Vec::new()))
}
