//! One page of a record listing, walked from a cursor's rank. Ported from
//! the `LeaderboardRecordsList` row loop: a previous-page cursor reads the
//! rows in reverse, so the page is flipped back and its two cursors with it,
//! each cursor keeping the direction it was built with.

use chrono::{DateTime, Utc};

use crate::dto::leaderboard::RecordResponse;
use crate::error::V2Error;
use crate::repositories::db::Queries;
use crate::repositories::leaderboard::{
    FirstLeaderboardRecordsAscParams, GreaterLeaderboardRecordsAscParams,
};
use crate::repositories::models::{Leaderboard, LeaderboardRecord};
use crate::utils::leaderboard::cursor::Cursor;
use crate::utils::leaderboard::types::{LIST_FAILED, ascending};

#[derive(Default)]
pub(super) struct Page {
    pub(super) records: Vec<RecordResponse>,
    pub(super) next_cursor: String,
    pub(super) prev_cursor: String,
}

/// Ascending forward and descending backward share one query, and so do the
/// other two directions: the ordering is always the one the page reads in.
async fn fetch(
    queries: &Queries,
    board: &Leaderboard,
    window: DateTime<Utc>,
    limit: i64,
    cursor: Option<&Cursor>,
) -> Result<Vec<LeaderboardRecord>, V2Error> {
    let ascending = ascending(board);
    let page = FirstLeaderboardRecordsAscParams {
        leaderboard_id: board.id.clone(),
        expiry_time: window,
        limit: limit + 1,
    };
    let query = match cursor {
        None => {
            if ascending {
                queries.first_leaderboard_records_asc(&page).await
            } else {
                queries.first_leaderboard_records_desc(&page).await
            }
        }
        Some(cursor) => {
            let bounds = GreaterLeaderboardRecordsAscParams {
                leaderboard_id: board.id.clone(),
                expiry_time: window,
                score: cursor.score,
                subscore: cursor.subscore,
                owner_id: cursor.owner()?,
                limit: limit + 1,
            };
            if (ascending && cursor.is_next) || (!ascending && !cursor.is_next) {
                queries.greater_leaderboard_records_asc(&bounds).await
            } else {
                queries.less_leaderboard_records_desc(&bounds).await
            }
        }
    };
    query.map_err(|error| {
        eprintln!("[v2] leaderboard records page: {error}");
        V2Error::internal(LIST_FAILED)
    })
}

pub(super) async fn record_page(
    queries: &Queries,
    board: &Leaderboard,
    window: DateTime<Utc>,
    limit: i64,
    cursor: Option<&Cursor>,
) -> Result<Page, V2Error> {
    let expiry = window.timestamp();
    let rows = fetch(queries, board, window, limit, cursor).await?;
    let mut rank = cursor.map_or(0, |c| c.rank);
    let mut records: Vec<RecordResponse> = Vec::with_capacity(limit as usize);
    let mut last: Option<(LeaderboardRecord, i64)> = None;
    let mut next: Option<Cursor> = None;
    let mut prev: Option<Cursor> = None;

    for row in rows {
        if records.len() as i64 >= limit {
            if let Some((seen, seen_rank)) = &last {
                next = Some(Cursor::at_record(&board.id, expiry, seen, *seen_rank, true));
            }
            break;
        }
        rank += if cursor.is_some_and(|c| !c.is_next) {
            -1
        } else {
            1
        };
        if cursor.is_some() && prev.is_none() {
            prev = Some(Cursor::at_record(&board.id, expiry, &row, rank, false));
        }
        last = Some((row.clone(), rank));
        records.push(RecordResponse::from_record(&row, rank));
    }

    if cursor.is_some_and(|c| !c.is_next) {
        match (next.take(), prev.take()) {
            (Some(n), Some(p)) => {
                next = Some(p);
                prev = Some(n);
            }
            (Some(mut n), None) => {
                n.is_next = false;
                prev = Some(n);
            }
            (None, Some(mut p)) => {
                p.is_next = true;
                next = Some(p);
            }
            (None, None) => {}
        }
        records.reverse();
    }

    Ok(Page {
        records,
        next_cursor: next.map(|c| c.encode()).transpose()?.unwrap_or_default(),
        prev_cursor: prev.map(|c| c.encode()).transpose()?.unwrap_or_default(),
    })
}
