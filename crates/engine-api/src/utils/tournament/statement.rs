use chrono::{DateTime, Utc};
use serde_json::Value;
use uuid::Uuid;

use crate::repositories::models::LeaderboardRecord;
use crate::utils::leaderboard::{DB_DECREMENT, DB_INCREMENT, DB_SET};

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

pub(crate) fn next_score(
    write: &Write<'_>,
    score: i64,
    subscore: i64,
    descending: bool,
) -> (i64, i64) {
    match write.operator {
        DB_SET => (write.delta.0, write.delta.1),
        DB_INCREMENT => (score + write.delta.0, subscore + write.delta.1),
        DB_DECREMENT => (
            (score - write.delta.0).max(0),
            (subscore - write.delta.1).max(0),
        ),
        _ if descending => (score.max(write.delta.0), subscore.max(write.delta.1)),
        _ => (score.min(write.delta.0), subscore.min(write.delta.1)),
    }
}

pub(crate) fn entered_row(write: &Write<'_>, now: DateTime<Utc>) -> LeaderboardRecord {
    LeaderboardRecord {
        leaderboard_id: write.leaderboard_id.to_string(),
        owner_id: write.owner_id,
        username: write.username.map(str::to_string),
        score: write.abs.0,
        subscore: write.abs.1,
        num_score: 1,
        metadata: write
            .metadata
            .clone()
            .unwrap_or_else(|| serde_json::json!({})),
        create_time: now,
        update_time: now,
        expiry_time: write.expiry,
        max_num_score: write.max_num_score,
    }
}

pub(crate) fn joined_row(
    leaderboard_id: &str,
    owner_id: Uuid,
    username: &str,
    expiry: DateTime<Utc>,
    max_num_score: i32,
    now: DateTime<Utc>,
) -> LeaderboardRecord {
    LeaderboardRecord {
        leaderboard_id: leaderboard_id.to_string(),
        owner_id,
        username: Some(username.to_string()),
        score: 0,
        subscore: 0,
        num_score: 0,
        metadata: serde_json::json!({}),
        create_time: now,
        update_time: now,
        expiry_time: expiry,
        max_num_score,
    }
}
