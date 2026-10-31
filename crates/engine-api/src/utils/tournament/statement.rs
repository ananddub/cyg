use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use crate::repositories::models::LeaderboardRecord;
use crate::utils::leaderboard::{DB_DECREMENT, DB_INCREMENT, DB_SET};

const PAGE_BASE: &str = "SELECT * FROM leaderboard
WHERE duration != 0
  AND category >= $category_start AND category <= $category_end
  AND start_time >= $start_from
  AND (
    ($mode = 0 AND (end_time = $never OR end_time >= $now))
    OR ($mode = 1 AND end_time = $never)
    OR ($mode = 2 AND end_time != $never AND end_time <= $end_bound)
  )";

const PAGE_CURSOR: &str = "  AND (
    start_time > $anchor_start
    OR (start_time = $anchor_start AND end_time < $anchor_end)
    OR (start_time = $anchor_start AND end_time = $anchor_end AND category > $anchor_category)
    OR (start_time = $anchor_start AND end_time = $anchor_end
        AND category = $anchor_category AND id > type::record('leaderboard', $anchor_id))
  )";

const PAGE_ORDER: &str = "ORDER BY start_time ASC, end_time DESC, category ASC, id ASC LIMIT $limit";

pub(crate) fn page_sql(has_anchor: bool) -> String {
    if has_anchor {
        format!("{PAGE_BASE}\n{PAGE_CURSOR}\n{PAGE_ORDER}")
    } else {
        format!("{PAGE_BASE}\n{PAGE_ORDER}")
    }
}

pub(crate) const EXISTS_SQL: &str = "SELECT * FROM leaderboard_record
WHERE leaderboard_id = $leaderboard_id AND owner_id = $owner_id
  AND expiry_time = $expiry_time LIMIT 1";

pub(crate) const DELETE_SQL: &str = "DELETE leaderboard_record
WHERE leaderboard_id = $leaderboard_id AND owner_id = $owner_id
  AND expiry_time = $expiry_time RETURN BEFORE";

pub(crate) const JOIN_SIZE_SQL: &str =
    "UPDATE type::record('leaderboard', $leaderboard_id) SET size = size + 1 \
     WHERE size < max_size RETURN AFTER";

pub(crate) const RELEASE_SIZE_SQL: &str =
    "UPDATE type::record('leaderboard', $leaderboard_id) SET size = size - 1";

pub(crate) const UPDATE_SQL: &str = "UPDATE leaderboard_record
SET score = $score, subscore = $subscore,
    num_score = num_score + 1,
    metadata = $metadata, username = $username, update_time = $update_time
WHERE leaderboard_id = $leaderboard_id AND owner_id = $owner_id
  AND expiry_time = $expiry_time
  AND (max_num_score = 0 OR num_score < max_num_score)
RETURN AFTER";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct Attempts {
    pub num_score: i32,
    pub max_num_score: i32,
}

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
        metadata: write.metadata.clone().unwrap_or_else(|| serde_json::json!({})),
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
