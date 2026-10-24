//! `PUT`/`POST /v2/tournament/{tournament_id}` — submit a score to the round.
//!
//! The body is the record message itself, as the gateway binds it. A board that requires a
//! join only updates the row the join left behind; only the insert path moves the size counter.

use axum::Json;
use chrono::{DateTime, Utc};
use serde_json::Value;
use uuid::Uuid;

use super::load_tournament;
use super::submit::submit;
use crate::ApiState;
use crate::app::middleware::Authed;
use crate::dto::leaderboard::{RecordRequest, RecordResponse};
use crate::error::V2Error;
use crate::repositories::db::Queries;
use crate::repositories::leaderboard::GetLeaderboardRecordParams;
use crate::repositories::models::Leaderboard;
use crate::services::auth::q;
use crate::services::leaderboard::{pool, rank_of};
use crate::utils::leaderboard::{DB_DECREMENT, at, calculate, parse_metadata, ranks_enabled};
use crate::utils::tournament::statement::Write;
use crate::utils::tournament::types::{
    AUTH_WRITE, BAD_RECORD, ID_MISSING_DOT, INACTIVE_WRITE, NOT_FOUND, WRITE_ENDED, WRITE_FAILED,
    db_operator,
};

/// The gateway binds the request body into the `record` field, so a body that
/// names nothing is a request with no record at all.
fn record_of(body: Value) -> Result<RecordRequest, V2Error> {
    if body.is_null() || body.as_object().is_some_and(|map| map.is_empty()) {
        return Err(V2Error::invalid_argument(BAD_RECORD));
    }
    serde_json::from_value(body).map_err(|_| V2Error::invalid_argument(BAD_RECORD))
}

/// What a write answers with: the row as it now stands, ranked when the board
/// keeps ranks. A write that changed nothing answers the same way.
async fn read_back(
    queries: &Queries,
    board: &Leaderboard,
    owner: Uuid,
    expiry: DateTime<Utc>,
) -> Result<Json<RecordResponse>, V2Error> {
    let record = queries
        .get_leaderboard_record(&GetLeaderboardRecordParams {
            leaderboard_id: board.id.clone(),
            owner_id: owner,
            expiry_time: expiry,
        })
        .await
        .map_err(|error| {
            eprintln!("[v2] tournament write read-back: {error}");
            V2Error::internal(WRITE_FAILED)
        })?;
    let rank = if ranks_enabled(board) {
        rank_of(queries, board, &record).await.map_err(|error| {
            eprintln!("[v2] tournament write rank: {error}");
            V2Error::internal(WRITE_FAILED)
        })?
    } else {
        0
    };
    Ok(Json(RecordResponse::from_record(&record, rank)))
}

impl super::TournamentService {
    pub async fn write_record(
        &self,
        state: &ApiState,
        auth: &Authed,
        id: &str,
        body: Value,
    ) -> Result<Json<RecordResponse>, V2Error> {
        if id.is_empty() {
            return Err(V2Error::invalid_argument(ID_MISSING_DOT));
        }
        let input = record_of(body)?;
        let metadata = match input.metadata.as_deref() {
            Some(raw) if !raw.is_empty() => Some(parse_metadata(raw)?),
            _ => None,
        };

        let queries = q(state)?;
        let board = match load_tournament(&queries, id).await {
            Ok(board) => board,
            Err(_) => return Err(V2Error::not_found(NOT_FOUND)),
        };
        let now = Utc::now();
        let end = board.end_time.timestamp();
        if end > 0 && end <= now.timestamp() {
            return Err(V2Error::not_found(WRITE_ENDED));
        }
        if board.authoritative {
            return Err(V2Error::permission_denied(AUTH_WRITE));
        }

        let deadlines = calculate(&board, now)?;
        if deadlines.start_active > now.timestamp() || deadlines.end_active <= now.timestamp() {
            return Err(V2Error::failed_precondition(INACTIVE_WRITE));
        }
        let operator = match db_operator(input.operator)? {
            None => board.operator,
            Some(operator) => operator,
        };
        let abs = if operator == DB_DECREMENT {
            (0, 0)
        } else {
            (input.score, input.subscore)
        };
        let write = Write {
            leaderboard_id: &board.id,
            owner_id: auth.uid(),
            username: (!auth.0.un.is_empty()).then_some(auth.0.un.as_str()),
            delta: (input.score, input.subscore),
            abs,
            metadata,
            expiry: at(deadlines.expiry),
            max_num_score: board.max_num_score,
            operator,
        };

        submit(pool(state)?, &board, &write).await?;
        read_back(&queries, &board, write.owner_id, write.expiry).await
    }
}
