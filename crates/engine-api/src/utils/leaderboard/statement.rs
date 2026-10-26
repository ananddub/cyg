//! The per-operator upsert, hand-written because FlashORM cannot shape one
//! statement per operator.
//!
//! One statement shape serves every operator: the insert writes the absolute
//! score, the `DO UPDATE` half differs. A decrement inserts a zeroed row and only
//! subtracts on conflict; a best write compares in the board's sort direction.
//! Increment binds its delta twice: `$4`/`$5` insert and `$8`/`$9` add.

use chrono::{DateTime, Utc};
use serde_json::Value;
use uuid::Uuid;

use super::types::{DB_DECREMENT, DB_INCREMENT, DB_SET};

const INSERT: &str = "INSERT INTO leaderboard_record
    (leaderboard_id, owner_id, username, score, subscore, metadata, expiry_time)
    VALUES ($1, $2, $3, ";

const INSERT_TAIL: &str = ", COALESCE($6, '{}'::JSONB), $7)";

/// A decrement inserts a zeroed row: only the update path subtracts, reading the
/// same two binds the other operators insert verbatim.
const ZEROED: &str = "0, 0";

/// A board's `abs` values build the row, its `delta` values feed the operator.
pub(crate) struct Write<'a> {
    pub(crate) leaderboard_id: &'a str,
    pub(crate) owner_id: Uuid,
    pub(crate) username: Option<&'a str>,
    pub(crate) abs: (i64, i64),
    pub(crate) delta: (i64, i64),
    pub(crate) metadata: Option<Value>,
    pub(crate) expiry: DateTime<Utc>,
    pub(crate) operator: i16,
}

fn clause(operator: i16, descending: bool) -> (String, String) {
    match operator {
        DB_SET => (
            "score = $4, subscore = $5".to_string(),
            " WHERE leaderboard_record.score <> $4 OR leaderboard_record.subscore <> $5".to_string(),
        ),
        DB_INCREMENT => (
            "score = leaderboard_record.score + $8, subscore = leaderboard_record.subscore + $9"
                .to_string(),
            " WHERE $8 <> 0 OR $9 <> 0".to_string(),
        ),
        DB_DECREMENT => (
            "score = GREATEST(leaderboard_record.score - $4, 0), subscore = GREATEST(leaderboard_record.subscore - $5, 0)".to_string(),
            " WHERE $4 <> 0 OR $5 <> 0".to_string(),
        ),
        _ if descending => (
            "score = GREATEST(leaderboard_record.score, $4), subscore = GREATEST(leaderboard_record.subscore, $5)".to_string(),
            " WHERE leaderboard_record.score < $4 OR leaderboard_record.subscore < $5".to_string(),
        ),
        _ => (
            "score = LEAST(leaderboard_record.score, $4), subscore = LEAST(leaderboard_record.subscore, $5)".to_string(),
            " WHERE leaderboard_record.score > $4 OR leaderboard_record.subscore > $5".to_string(),
        ),
    }
}

pub(crate) fn statement(write: &Write<'_>, descending: bool) -> String {
    let (op, filter) = clause(write.operator, descending);
    let values = if write.operator == DB_DECREMENT {
        ZEROED
    } else {
        "$4, $5"
    };
    format!(
        "{INSERT}{values}{INSERT_TAIL}
         ON CONFLICT (owner_id, leaderboard_id, expiry_time)
         DO UPDATE SET {op},
             num_score = leaderboard_record.num_score + 1,
             metadata = COALESCE($6, leaderboard_record.metadata),
             username = COALESCE($3, leaderboard_record.username),
             update_time = now(){filter}
         RETURNING *"
    )
}
