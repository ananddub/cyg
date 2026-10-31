use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use super::*;

/// One write of a score into a board: the entry it aims at, the values the
/// caller already decided, and when that entry expires.
pub struct TournamentEntryWrite<'a> {
    pub leaderboard_id: &'a str,
    pub owner_id: Uuid,
    pub username: Option<&'a str>,
    pub score: i64,
    pub subscore: i64,
    pub metadata: Value,
    pub expiry: DateTime<Utc>,
    pub now: DateTime<Utc>,
}

/// Where a page of boards starts and how far it may run, with the board the
/// last page ended on when the caller is continuing.
pub struct TournamentPage<'a> {
    pub category_start: i16,
    pub category_end: i16,
    pub start_from: DateTime<Utc>,
    pub mode: i32,
    pub now: DateTime<Utc>,
    pub end_bound: DateTime<Utc>,
    pub limit: i64,
    pub anchor: Option<&'a Leaderboard>,
}

/// The attempt counters an update hands back, which is how a write learns the
/// entry it just changed had already spent its tries.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Attempts {
    pub num_score: i32,
    pub max_num_score: i32,
}
