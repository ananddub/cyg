//! The haystack's two halves and the window that trims them.
//!
//! Ported from `getLeaderboardRecordsHaystack`: the owner's own record anchors the read,
//! the better half is fetched away from the owner and reversed, the worse half is capped.

use chrono::{DateTime, Utc};
use uuid::Uuid;

use super::rank::{rank_count, ranks};
use crate::dto::leaderboard::{RecordListResponse, RecordResponse};
use crate::error::V2Error;
use crate::repositories::db::Queries;
use crate::repositories::leaderboard::{
    GetLeaderboardRecordParams, GreaterLeaderboardRecordsAscParams,
};
use crate::repositories::models::{Leaderboard, LeaderboardRecord};
use crate::utils::leaderboard::cursor::Cursor;
use crate::utils::leaderboard::types::HAYSTACK_FAILED;
use crate::utils::leaderboard::types::ascending;

pub(super) fn failed(error: sqlx::Error) -> V2Error {
    tracing::error!("[v2] leaderboard haystack: {error}");
    V2Error::internal(HAYSTACK_FAILED)
}

/// A haystack for a player who has never scored is an empty listing, not an
/// error, and not a missing board either.
async fn owner_record(
    queries: &Queries,
    board: &Leaderboard,
    window: DateTime<Utc>,
    owner: Uuid,
) -> Result<Option<LeaderboardRecord>, V2Error> {
    let params = GetLeaderboardRecordParams {
        leaderboard_id: board.id.clone(),
        owner_id: owner,
        expiry_time: window,
    };
    match queries.get_leaderboard_record(&params).await {
        Ok(record) => Ok(Some(record)),
        Err(sqlx::Error::RowNotFound) => Ok(None),
        Err(error) => Err(failed(error)),
    }
}

/// One side of the owner. The better half is read in the order away from the
/// owner so the closest records land last, and is reversed by the caller.
async fn half(
    queries: &Queries,
    board: &Leaderboard,
    bounds: &GreaterLeaderboardRecordsAscParams,
    better: bool,
) -> Result<Vec<LeaderboardRecord>, V2Error> {
    let rows = match (ascending(board), better) {
        (true, true) | (false, false) => queries.less_leaderboard_records_desc(bounds).await,
        _ => queries.greater_leaderboard_records_asc(bounds).await,
    };
    rows.map_err(failed)
}

pub(crate) async fn haystack(
    queries: &Queries,
    board: &Leaderboard,
    window: DateTime<Utc>,
    owner: Uuid,
    limit: i64,
) -> Result<RecordListResponse, V2Error> {
    let Some(own) = owner_record(queries, board, window, owner).await? else {
        return Ok(RecordListResponse::default());
    };
    let bounds = |take: i64| GreaterLeaderboardRecordsAscParams {
        leaderboard_id: board.id.clone(),
        expiry_time: window,
        score: own.score,
        subscore: own.subscore,
        owner_id: owner,
        limit: take,
    };

    let mut better = half(queries, board, &bounds(limit + 1), true).await?;
    let mut set_prev = false;
    if better.len() as i64 > limit {
        set_prev = true;
        better.pop();
    }
    better.reverse();

    let mut worse_limit = limit / 2;
    if (better.len() as i64) < worse_limit {
        worse_limit = limit - better.len() as i64;
    }
    let mut worse = half(queries, board, &bounds(worse_limit + 1), false).await?;
    let mut set_next = false;
    if worse.len() as i64 > worse_limit {
        set_next = true;
        worse.pop();
    }

    let better_len = better.len() as i64;
    let mut rows = better;
    rows.push(own);
    rows.extend(worse);

    let total = rows.len() as i64;
    let mut start = total - limit;
    if start < 0 || better_len < worse_limit {
        start = 0;
    }
    let end = (start + limit).min(total);
    set_prev |= start > 0;
    let rows = &rows[start as usize..end as usize];

    let ranks = ranks(queries, board, rows).await.map_err(failed)?;
    let records: Vec<RecordResponse> = rows
        .iter()
        .zip(&ranks)
        .map(|(row, rank)| RecordResponse::from_record(row, *rank))
        .collect();
    let expiry = window.timestamp();
    let last = rows.len() - 1;
    Ok(RecordListResponse {
        records,
        next_cursor: set_next
            .then(|| Cursor::at_record(&board.id, expiry, &rows[last], ranks[last], true))
            .map(|cursor| cursor.encode())
            .transpose()?
            .unwrap_or_default(),
        prev_cursor: set_prev
            .then(|| Cursor::at_record(&board.id, expiry, &rows[0], ranks[0], false))
            .map(|cursor| cursor.encode())
            .transpose()?
            .unwrap_or_default(),
        owner_records: Vec::new(),
        rank_count: rank_count(queries, board, window).await?,
    })
}
