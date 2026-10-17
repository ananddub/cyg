//! Matchmaker: BaaS `server/matchmaker.go` parity in miniature.
//! Tickets pool in memory; a 5-second sweep groups compatible ones into a relay match.

use std::collections::HashMap;
use std::sync::{OnceLock, RwLock};
use uuid::Uuid;

use chrono::{DateTime, Utc};

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
    created_at: DateTime<Utc>,
}

static POOL: OnceLock<RwLock<Vec<Ticket>>> = OnceLock::new();
static TICKER_ONCE: OnceLock<()> = OnceLock::new();

fn pool() -> &'static RwLock<Vec<Ticket>> {
    POOL.get_or_init(|| RwLock::new(Vec::new()))
}

/// The queue as the stats route reports it: how many tickets are waiting overall,
/// how long the oldest has waited, and the same split per matchset. A matchset is
/// the ticket's query string, the one grouping a relay matchmaker can honour.
pub struct MatchsetStats {
    pub matchset: String,
    pub ticket_count: i64,
    pub oldest_create_time: DateTime<Utc>,
    pub avg_wait_secs: f64,
}

/// Snapshot of the ticket pool. Completion history is not kept, so only the queue
/// side of the reference server's stats has real data behind it.
pub struct MatchmakerStatsSnapshot {
    pub ticket_count: i64,
    pub oldest_ticket_create_time: Option<DateTime<Utc>>,
    pub matchsets: Vec<MatchsetStats>,
}

/// Read the pool once and fold it into per-matchset counts and wait averages.
pub fn stats() -> MatchmakerStatsSnapshot {
    let now = Utc::now();
    let all = pool().read().expect("matchmaker pool lock");
    let mut oldest: Option<DateTime<Utc>> = None;
    let mut groups: HashMap<&str, Vec<i64>> = HashMap::new();
    for ticket in all.iter() {
        let waited = (now - ticket.created_at).num_milliseconds();
        oldest = Some(oldest.map_or(ticket.created_at, |o| o.min(ticket.created_at)));
        groups
            .entry(ticket.query.as_str())
            .or_default()
            .push(waited);
    }
    let mut matchsets: Vec<MatchsetStats> = groups
        .into_iter()
        .map(|(matchset, waits)| MatchsetStats {
            matchset: matchset.to_owned(),
            ticket_count: waits.len() as i64,
            oldest_create_time: now - chrono::Duration::milliseconds(waits.iter().copied().min().unwrap_or(0)),
            avg_wait_secs: waits.iter().copied().sum::<i64>() as f64 / waits.len() as f64 / 1000.0,
        })
        .collect();
    matchsets.sort_by(|a, b| a.matchset.cmp(&b.matchset));
    MatchmakerStatsSnapshot {
        ticket_count: all.len() as i64,
        oldest_ticket_create_time: oldest,
        matchsets,
    }
}
