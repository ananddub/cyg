//! The tournament routes' statements, hand-written. They
//! run as raw sqlx because FlashORM names a parameter after the column it meets,
//! and every statement here meets one column twice. Ten binds, as BaaS has.

use chrono::{DateTime, Utc};
use serde_json::Value;
use uuid::Uuid;

use crate::utils::leaderboard::{DB_DECREMENT, DB_INCREMENT, DB_SET};

/// The cache `ListTournaments` walks as one query: $1/$2 category bounds, $3
/// earliest start, $4 end mode, $5 now, $6 end bound, $7-$11 cursor tuple, $12 limit.
pub(crate) const PAGE_SQL: &str = "SELECT * FROM leaderboard
WHERE duration <> 0
  AND category >= $1 AND category <= $2
  AND start_time >= $3
  AND (
    ($4 = 0 AND (end_time = 'epoch'::timestamptz OR end_time >= $5))
    OR ($4 = 1 AND end_time = 'epoch'::timestamptz)
    OR ($4 = 2 AND end_time <> 'epoch'::timestamptz AND end_time <= $6)
  )
  AND (
    NOT $7
    OR start_time > $8
    OR (start_time = $8 AND end_time < $9)
    OR (start_time = $8 AND end_time = $9 AND category > $10)
    OR (start_time = $8 AND end_time = $9 AND category = $10 AND id > $11)
  )
ORDER BY start_time ASC, end_time DESC, category ASC, id ASC
LIMIT $12";

pub(crate) const JOIN_SQL: &str = "INSERT INTO leaderboard_record
    (leaderboard_id, owner_id, expiry_time, username, num_score, max_num_score)
VALUES ($1, $2, $3, $4, 0, $5)
ON CONFLICT (owner_id, leaderboard_id, expiry_time) DO NOTHING";

pub(crate) const JOIN_SIZE_SQL: &str =
    "UPDATE leaderboard SET size = size + 1 WHERE id = $1 AND size < max_size";

pub(crate) const RELEASE_SIZE_SQL: &str = "UPDATE leaderboard SET size = size - 1 WHERE id = $1";

pub(crate) const EXISTS_SQL: &str = "SELECT 1 FROM leaderboard_record
WHERE leaderboard_id = $1 AND owner_id = $2 AND expiry_time = $3";

pub(crate) const DELETE_SQL: &str = "DELETE FROM leaderboard_record
WHERE leaderboard_id = $1 AND owner_id = $2 AND expiry_time = $3";

const INSERT_HEAD: &str = "INSERT INTO leaderboard_record
    (leaderboard_id, owner_id, username, score, subscore, metadata, expiry_time, max_num_score)
VALUES ($1, $2, $3, $9, $10, COALESCE($7, '{}'::JSONB), $4, $8)
ON CONFLICT (owner_id, leaderboard_id, expiry_time)
DO UPDATE SET ";

const INSERT_TAIL: &str = ", num_score = leaderboard_record.num_score + 1,
    metadata = COALESCE($7, leaderboard_record.metadata),
    username = COALESCE($3, leaderboard_record.username),
    update_time = now()
RETURNING num_score, max_num_score";

const UPDATE_HEAD: &str = "UPDATE leaderboard_record
SET ";

const UPDATE_TAIL: &str = ", num_score = leaderboard_record.num_score + 1,
    metadata = COALESCE($7, leaderboard_record.metadata),
    username = COALESCE($3, leaderboard_record.username),
    update_time = now()
WHERE leaderboard_id = $1 AND owner_id = $2 AND expiry_time = $4
  AND (max_num_score = 0 OR num_score < max_num_score)";

/// What one write supplies: the board's operator, its deltas at `$5`/`$6` and the absolutes a fresh row takes.
pub(crate) struct Write<'a> {
    pub(crate) leaderboard_id: &'a str,
    pub(crate) owner_id: Uuid,
    pub(crate) username: Option<&'a str>,
    pub(crate) delta: (i64, i64),
    pub(crate) abs: (i64, i64),
    pub(crate) metadata: Option<Value>,
    pub(crate) expiry: DateTime<Utc>,
    pub(crate) max_num_score: i32,
    pub(crate) operator: i16,
}

/// The score half of both statements: a decrement subtracts and floors at zero,
/// a best write compares in the board's direction.
fn clause(operator: i16, descending: bool) -> String {
    match operator {
        DB_SET => "score = $5, subscore = $6".to_string(),
        DB_INCREMENT => {
            "score = leaderboard_record.score + $5, subscore = leaderboard_record.subscore + $6"
                .to_string()
        }
        DB_DECREMENT => "score = GREATEST(leaderboard_record.score - $5, 0), subscore = GREATEST(leaderboard_record.subscore - $6, 0)".to_string(),
        _ if descending => {
            "score = GREATEST(leaderboard_record.score, $5), subscore = GREATEST(leaderboard_record.subscore, $6)".to_string()
        }
        _ => "score = LEAST(leaderboard_record.score, $5), subscore = LEAST(leaderboard_record.subscore, $6)".to_string(),
    }
}

/// The path for a board anyone may enter: insert, or update the row there, answering with the attempt counters.
pub(crate) fn accept(write: &Write<'_>, descending: bool) -> String {
    format!(
        "{INSERT_HEAD}{}{INSERT_TAIL}",
        clause(write.operator, descending)
    )
}

/// The path for a board requiring a join: only a row the join already put there is written; no size counter moves.
pub(crate) fn accept_joined(write: &Write<'_>, descending: bool) -> String {
    format!(
        "{UPDATE_HEAD}{}{UPDATE_TAIL}",
        clause(write.operator, descending)
    )
}
