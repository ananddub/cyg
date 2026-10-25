//! `GET /v2/leaderboard/{leaderboard_id}` — a page of records, optionally plus a
//! batch of named owners' records. Ported from the
//! `server/core_leaderboard.go` `LeaderboardRecordsList`.

use axum::Json;
use chrono::{DateTime, Utc};
use uuid::Uuid;

use super::load_board;
use super::page::{Page, record_page};
use super::rank::{rank_count, wires};
use crate::ApiState;
use crate::app::middleware::Authed;
use crate::dto::leaderboard::{ListQuery, RecordListResponse, RecordResponse};
use crate::error::V2Error;
use crate::repositories::db::Queries;
use crate::repositories::leaderboard::ListLeaderboardRecordsForOwnerParams;
use crate::repositories::models::Leaderboard;
use crate::services::auth::q;
use crate::utils::leaderboard::cursor::Cursor;
use crate::utils::leaderboard::types::{
    BAD_EXPIRY, BAD_OWNERS, INVALID_ID, LIST_FAILED, LIST_LIMIT, MAX_LIST_LIMIT, ascending, number,
    owner_ids,
};
use crate::utils::leaderboard::window::query_window;

/// The gateway's page size: absent means one record, unless the caller named
/// owners without paging, which asks for owner records only.
fn requested_limit(
    raw: Option<&str>,
    cursor: &str,
    has_owners: bool,
) -> Result<Option<i64>, V2Error> {
    if let Some(text) = raw {
        let limit = number(Some(text), LIST_LIMIT)?;
        return if (1..=MAX_LIST_LIMIT).contains(&limit) {
            Ok(Some(limit))
        } else {
            Err(V2Error::invalid_argument(LIST_LIMIT))
        };
    }
    Ok(if cursor.is_empty() && has_owners {
        None
    } else {
        Some(1)
    })
}

impl super::LeaderboardService {
    pub async fn list_records(
        &self,
        state: &ApiState,
        _auth: &Authed,
        id: &str,
        req: ListQuery,
    ) -> Result<Json<RecordListResponse>, V2Error> {
        if id.is_empty() {
            return Err(V2Error::invalid_argument(INVALID_ID));
        }
        let owners = owner_ids(req.owner_ids.as_ref(), BAD_OWNERS)?;
        let limit = requested_limit(req.limit.as_deref(), &req.cursor, !owners.is_empty())?;
        let override_expiry = number(req.expiry.as_deref(), BAD_EXPIRY)?;

        let queries = q(state)?;
        let board = load_board(&queries, id).await?;
        let Some(window) = query_window(&board, override_expiry)? else {
            return Ok(Json(RecordListResponse::default()));
        };
        Ok(Json(
            records_list(&queries, &board, window, limit, &req.cursor, &owners).await?,
        ))
    }
}

/// The listing itself, shared by the route above and the haystack route once handed a
/// cursor, and reused by the tournament group (so it takes the query shape, not state).
pub(crate) async fn records_list(
    queries: &Queries,
    board: &Leaderboard,
    window: DateTime<Utc>,
    limit: Option<i64>,
    cursor: &str,
    owners: &[Uuid],
) -> Result<RecordListResponse, V2Error> {
    let mut listed = Page::default();
    if let Some(limit) = limit {
        let incoming = Cursor::decode(cursor, &board.id, window.timestamp())?;
        listed = record_page(queries, board, window, limit, incoming.as_ref()).await?;
    }
    Ok(RecordListResponse {
        records: listed.records,
        next_cursor: listed.next_cursor,
        prev_cursor: listed.prev_cursor,
        owner_records: if owners.is_empty() {
            Vec::new()
        } else {
            owner_records(queries, board, window, owners).await?
        },
        rank_count: rank_count(queries, board, window).await?,
    })
}

/// Named owners read unordered and are sorted to the board's direction, since
/// they are a batch lookup rather than a page.
async fn owner_records(
    queries: &Queries,
    board: &Leaderboard,
    window: DateTime<Utc>,
    owners: &[Uuid],
) -> Result<Vec<RecordResponse>, V2Error> {
    let mut rows = queries
        .list_leaderboard_records_for_owner(&ListLeaderboardRecordsForOwnerParams {
            leaderboard_id: board.id.clone(),
            expiry_time: window,
            owner_id: owners.to_vec(),
        })
        .await
        .map_err(|error| {
            tracing::error!("[v2] leaderboard owner records: {error}");
            V2Error::internal(LIST_FAILED)
        })?;
    let ascending = ascending(board);
    rows.sort_by(|a, b| {
        let order = a
            .score
            .cmp(&b.score)
            .then(a.subscore.cmp(&b.subscore))
            .then(a.owner_id.cmp(&b.owner_id));
        if ascending { order } else { order.reverse() }
    });
    wires(queries, board, &rows).await
}
