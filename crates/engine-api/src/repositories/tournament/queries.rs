use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use serde_json::{Value, json};
use uuid::Uuid;

use super::*;
use crate::db::time;
use crate::repositories::db::ts;

const ENTRY_SQL: &str = "SELECT * FROM leaderboard_record
WHERE leaderboard_id = $leaderboard_id AND owner_id = $owner_id
  AND expiry_time = $expiry_time LIMIT 1";

const DELETE_ENTRY_SQL: &str = "DELETE leaderboard_record
WHERE leaderboard_id = $leaderboard_id AND owner_id = $owner_id
  AND expiry_time = $expiry_time RETURN BEFORE";

const GROW_SIZE_SQL: &str = "UPDATE type::record('leaderboard', $leaderboard_id) SET size = size + 1 \
     WHERE size < max_size RETURN AFTER";

const SHRINK_SIZE_SQL: &str =
    "UPDATE type::record('leaderboard', $leaderboard_id) SET size = size - 1";

const UPDATE_ENTRY_SQL: &str = "UPDATE leaderboard_record
SET score = $score, subscore = $subscore,
    num_score = num_score + 1,
    metadata = $metadata, username = $username, update_time = $update_time
WHERE leaderboard_id = $leaderboard_id AND owner_id = $owner_id
  AND expiry_time = $expiry_time
  AND (max_num_score = 0 OR num_score < max_num_score)
RETURN AFTER";

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

const PAGE_ORDER: &str =
    "ORDER BY start_time ASC, end_time DESC, category ASC, id ASC LIMIT $limit";

fn page_sql(has_anchor: bool) -> String {
    if has_anchor {
        format!("{PAGE_BASE}\n{PAGE_CURSOR}\n{PAGE_ORDER}")
    } else {
        format!("{PAGE_BASE}\n{PAGE_ORDER}")
    }
}

fn entry_params(
    leaderboard_id: &str,
    owner_id: Uuid,
    expiry: DateTime<Utc>,
) -> BTreeMap<String, Value> {
    params!(
        leaderboard_id = leaderboard_id,
        owner_id = owner_id.to_string(),
        expiry_time = ts(expiry),
    )
}

impl Queries {
    pub async fn get_tournament(&self, id: &str) -> Result<Leaderboard, DbError> {
        self.one(
            "SELECT * FROM type::record('leaderboard', $id) WHERE duration != 0 LIMIT 1",
            params!(id = id),
        )
        .await
    }

    /// This account's entry in this board for the window ending at `expiry`.
    pub async fn tournament_entry(
        &self,
        leaderboard_id: &str,
        owner_id: Uuid,
        expiry: DateTime<Utc>,
    ) -> Result<Option<LeaderboardRecord>, DbError> {
        self.opt(ENTRY_SQL, entry_params(leaderboard_id, owner_id, expiry))
            .await
    }

    /// Writes one score over an entry that still has tries left, handing back
    /// the counters it ended up with.
    pub async fn update_tournament_entry(
        &self,
        write: &TournamentEntryWrite<'_>,
    ) -> Result<Option<Attempts>, DbError> {
        let mut binds = entry_params(write.leaderboard_id, write.owner_id, write.expiry);
        binds.insert("score".to_string(), json!(write.score));
        binds.insert("subscore".to_string(), json!(write.subscore));
        binds.insert("metadata".to_string(), json!(write.metadata));
        binds.insert("username".to_string(), json!(write.username));
        binds.insert("update_time".to_string(), ts(write.now));
        self.opt(UPDATE_ENTRY_SQL, binds).await
    }

    /// Takes one place in a board, answering nothing when there is no place
    /// left to take.
    pub async fn grow_tournament_size(
        &self,
        leaderboard_id: &str,
    ) -> Result<Option<Leaderboard>, DbError> {
        self.opt(GROW_SIZE_SQL, params!(leaderboard_id = leaderboard_id))
            .await
    }

    pub async fn shrink_tournament_size(&self, leaderboard_id: &str) -> Result<(), DbError> {
        self.exec(SHRINK_SIZE_SQL, params!(leaderboard_id = leaderboard_id))
            .await
    }

    /// Drops the entry and answers the rows it removed, so the caller knows
    /// whether a place was really given back.
    pub async fn delete_tournament_entry(
        &self,
        leaderboard_id: &str,
        owner_id: Uuid,
        expiry: DateTime<Utc>,
    ) -> Result<Vec<LeaderboardRecord>, DbError> {
        self.all(
            DELETE_ENTRY_SQL,
            entry_params(leaderboard_id, owner_id, expiry),
        )
        .await
    }

    /// One page of the boards a player may discover, starting at `page.anchor`
    /// when the last page ended on one.
    pub async fn tournament_page(
        &self,
        page: &TournamentPage<'_>,
    ) -> Result<Vec<Leaderboard>, DbError> {
        let mut binds = params!(
            category_start = page.category_start,
            category_end = page.category_end,
            start_from = ts(page.start_from),
            mode = page.mode,
            now = ts(page.now),
            never = ts(time::epoch()),
            end_bound = ts(page.end_bound),
            limit = page.limit,
        );
        if let Some(anchor) = page.anchor {
            binds.insert("anchor_start".to_string(), ts(anchor.start_time));
            binds.insert("anchor_end".to_string(), ts(anchor.end_time));
            binds.insert("anchor_category".to_string(), json!(anchor.category));
            binds.insert("anchor_id".to_string(), json!(anchor.id));
        }
        self.all(&page_sql(page.anchor.is_some()), binds).await
    }
}
