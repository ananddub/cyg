//! `GET /v2/tournament` — the tournament browser.
//!
//! BaaS answers from its leaderboard cache, sorted by start, latest end, then category and id,
//! with a cursor naming the last tournament given out; an unknown cursor leaves an empty page.

use axum::Json;
use chrono::{DateTime, Utc};

use crate::dto::tournament::{ListQuery, TournamentListResponse, TournamentResponse};
use crate::utils::tournament::statement::PAGE_SQL;
use crate::utils::tournament::types::{
    BAD_CATEGORY_END, BAD_CATEGORY_ORDER, BAD_NUMBER, BAD_PAGE_LIMIT, BAD_TIME_ORDER,
    DEFAULT_CATEGORY_END, DEFAULT_PAGE_LIMIT, DEFAULT_TIME, PAGE_FAILED, decode_page_cursor,
    defaulted, limit, page_cursor,
};
use crate::repositories::models::Leaderboard;
use crate::services::auth::q;
use crate::error::V2Error;
use crate::services::leaderboard::pool;
use crate::utils::leaderboard::at;
use crate::app::middleware::Authed;
use crate::ApiState;

/// The filters, resolved the way the cache reads them: categories bound the column, a start
/// time of `-1` is no bound, and the end filter's mode picks which timestamp test is live.
struct Bounds {
    category_start: i16,
    category_end: i16,
    start_from: DateTime<Utc>,
    mode: i32,
    now: DateTime<Utc>,
    end_bound: DateTime<Utc>,
}

fn bounds(req: &ListQuery, now: DateTime<Utc>) -> Result<Bounds, V2Error> {
    let category_start = defaulted(req.category_start.as_deref(), 0, BAD_NUMBER)?;
    let category_end = defaulted(req.category_end.as_deref(), DEFAULT_CATEGORY_END, BAD_NUMBER)?;
    if req.category_end.is_some() && category_end >= 128 {
        return Err(V2Error::invalid_argument(BAD_CATEGORY_END));
    }
    if category_end < category_start {
        return Err(V2Error::invalid_argument(BAD_CATEGORY_ORDER));
    }

    let start_time = defaulted(req.start_time.as_deref(), DEFAULT_TIME, BAD_NUMBER)?;
    let end_time = defaulted(req.end_time.as_deref(), DEFAULT_TIME, BAD_NUMBER)?;
    if end_time != 0 && end_time < start_time {
        return Err(V2Error::invalid_argument(BAD_TIME_ORDER));
    }

    Ok(Bounds {
        category_start: small(category_start, BAD_CATEGORY_ORDER)?,
        category_end: small(category_end, BAD_CATEGORY_END)?,
        start_from: at(start_time),
        mode: if end_time == DEFAULT_TIME {
            0
        } else if end_time == 0 {
            1
        } else {
            2
        },
        now,
        end_bound: at(end_time),
    })
}

fn small(value: i64, message: &str) -> Result<i16, V2Error> {
    i16::try_from(value).map_err(|_| V2Error::invalid_argument(message))
}

impl super::TournamentService {
    pub async fn list_tournaments(
        &self,
        state: &ApiState,
        _auth: &Authed,
        req: ListQuery,
    ) -> Result<Json<TournamentListResponse>, V2Error> {
        let cursor_id = decode_page_cursor(&req.cursor)?;
        let limit = limit(req.limit.as_deref(), DEFAULT_PAGE_LIMIT, BAD_PAGE_LIMIT)?;

        let queries = q(state)?;
        let bounds = bounds(&req, Utc::now())?;

        let anchor = match cursor_id {
            None => None,
            Some(id) => match queries.get_tournament(&id).await {
                Ok(board) => Some(board),
                Err(sqlx::Error::RowNotFound) => return Ok(Json(TournamentListResponse::default())),
                Err(error) => {
                    eprintln!("[v2] tournament cursor: {error}");
                    return Err(V2Error::internal(PAGE_FAILED));
                }
            },
        };

        let rows = sqlx::query_as::<_, Leaderboard>(PAGE_SQL)
            .bind(bounds.category_start)
            .bind(bounds.category_end)
            .bind(bounds.start_from)
            .bind(bounds.mode)
            .bind(bounds.now)
            .bind(bounds.end_bound)
            .bind(anchor.is_some())
            .bind(anchor.as_ref().map(|board| board.start_time))
            .bind(anchor.as_ref().map(|board| board.end_time))
            .bind(anchor.as_ref().map(|board| board.category))
            .bind(anchor.as_ref().map(|board| board.id.as_str()))
            .bind(limit + 1)
            .fetch_all(pool(state)?)
            .await
            .map_err(|error| {
                eprintln!("[v2] tournament list: {error}");
                V2Error::internal(PAGE_FAILED)
            })?;

        Ok(Json(truncate(rows, limit)?))
    }
}

/// A page that came back full means there is more, and the cursor is the last
/// tournament the caller was given.
fn truncate(rows: Vec<Leaderboard>, limit: i64) -> Result<TournamentListResponse, V2Error> {
    let mut rows = rows;
    let has_more = rows.len() as i64 > limit;
    if has_more {
        rows.truncate(limit as usize);
    }
    let now = Utc::now();
    let cursor = match (has_more, rows.last()) {
        (true, Some(board)) => page_cursor(&board.id),
        _ => String::new(),
    };
    Ok(TournamentListResponse {
        tournaments: rows
            .iter()
            .map(|board| TournamentResponse::from_board(board, now))
            .collect::<Result<Vec<TournamentResponse>, V2Error>>()?,
        cursor,
    })
}
