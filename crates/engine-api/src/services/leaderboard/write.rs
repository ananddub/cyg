//! `POST /v2/leaderboard/{leaderboard_id}` — the body *is* the record message.
//!
//! The upsert runs with raw sqlx because a metadata bind may arrive as NULL, which
//! FlashORM's generated signatures do not allow. An authoritative board refuses a caller.

use axum::Json;
use serde_json::Value;
use sqlx::PgPool;

use crate::utils::leaderboard::statement::{Write, statement};
use crate::utils::leaderboard::types::{
    AUTH_WRITE, BAD_RECORD, DB_INCREMENT, INVALID_ID, WRITE_FAILED, ascending, parse_metadata,
    ranks_enabled, resolve_operator,
};
use crate::utils::leaderboard::window::record_window;
use crate::dto::leaderboard::{RecordRequest, RecordResponse};
use super::rank::rank_of;
use super::{load_board, pool};
use crate::repositories::db::Queries;
use crate::repositories::leaderboard::GetLeaderboardRecordParams;
use crate::repositories::models::{Leaderboard, LeaderboardRecord};
use crate::services::auth::q;
use crate::error::V2Error;
use crate::app::middleware::Authed;
use crate::ApiState;

/// A write that changed nothing answers with the record as it stands: the operator's
/// `WHERE` rejected it, or a concurrent write took the primary key slot BaaS aimed at.
async fn upsert(
    pool: &PgPool,
    queries: &Queries,
    write: &Write<'_>,
    sql: &str,
) -> Result<LeaderboardRecord, V2Error> {
    let mut query = sqlx::query_as::<_, LeaderboardRecord>(sql)
        .bind(write.leaderboard_id)
        .bind(write.owner_id)
        .bind(write.username)
        .bind(write.abs.0)
        .bind(write.abs.1)
        .bind(write.metadata.as_ref())
        .bind(write.expiry);
    if write.operator == DB_INCREMENT {
        query = query.bind(write.delta.0).bind(write.delta.1);
    }
    let written = query.fetch_optional(pool).await;
    let existing = GetLeaderboardRecordParams {
        leaderboard_id: write.leaderboard_id.to_string(),
        owner_id: write.owner_id,
        expiry_time: write.expiry,
    };
    match written {
        Ok(Some(record)) => Ok(record),
        Ok(None) => read_existing(queries, existing).await,
        Err(error) if V2Error::is_unique_violation(&error, "leaderboard_record_pkey") => {
            read_existing(queries, existing).await
        }
        Err(error) => {
            eprintln!("[v2] leaderboard write: {error}");
            Err(V2Error::internal(WRITE_FAILED))
        }
    }
}

async fn read_existing(
    queries: &Queries,
    params: GetLeaderboardRecordParams,
) -> Result<LeaderboardRecord, V2Error> {
    queries
        .get_leaderboard_record(&params)
        .await
        .map_err(|error| {
            eprintln!("[v2] leaderboard write read-back: {error}");
            V2Error::internal(WRITE_FAILED)
        })
}

/// The write's wire shape: the stored record, ranked when the board keeps ranks.
async fn apply(
    pool: &PgPool,
    queries: &Queries,
    board: &Leaderboard,
    write: &Write<'_>,
) -> Result<RecordResponse, V2Error> {
    let record = upsert(pool, queries, write, &statement(write, ascending(board))).await?;
    let rank = if ranks_enabled(board) {
        rank_of(queries, board, &record)
            .await
            .map_err(V2Error::from_sqlx)?
    } else {
        0
    };
    Ok(RecordResponse::from_record(&record, rank))
}

impl super::LeaderboardService {
    pub async fn write_record(
        &self,
        state: &ApiState,
        auth: &Authed,
        id: &str,
        body: Value,
    ) -> Result<Json<RecordResponse>, V2Error> {
        if id.is_empty() {
            return Err(V2Error::invalid_argument(INVALID_ID));
        }
        if body.is_null() {
            return Err(V2Error::invalid_argument(BAD_RECORD));
        }
        let input: RecordRequest =
            serde_json::from_value(body).map_err(|_| V2Error::invalid_argument(BAD_RECORD))?;
        let metadata = match input.metadata.as_deref() {
            Some(raw) if !raw.is_empty() => Some(parse_metadata(raw)?),
            _ => None,
        };

        let queries = q(state)?;
        let board = load_board(&queries, id).await?;
        if board.authoritative {
            return Err(V2Error::permission_denied(AUTH_WRITE));
        }
        let operator = resolve_operator(&board, input.operator)?;
        let username = (!auth.0.un.is_empty()).then_some(auth.0.un.as_str());
        let write = Write {
            leaderboard_id: &board.id,
            owner_id: auth.uid(),
            username,
            abs: (input.score, input.subscore),
            delta: (input.score, input.subscore),
            metadata,
            expiry: record_window(&board)?,
            operator,
        };
        Ok(Json(apply(pool(state)?, &queries, &board, &write).await?))
    }
}
