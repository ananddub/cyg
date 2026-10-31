use axum::Json;
use chrono::Utc;
use uuid::Uuid;

use super::load_tournament;
use crate::ApiState;
use crate::app::middleware::Authed;
use crate::dto::leaderboard::RecordListResponse;
use crate::dto::tournament::RecordsQuery;
use crate::error::V2Error;
use crate::repositories::db::Queries;
use crate::repositories::models::Leaderboard;
use crate::services::deps::q;
use crate::services::leaderboard::records_list;
use crate::utils::leaderboard::{number, query_window};
use crate::utils::tournament::types::limit as bounded;
use crate::utils::tournament::types::{
    BAD_EXPIRY, BAD_LIMIT, BAD_OWNERS, DEFAULT_RECORD_LIMIT, ENDED, ID_MISSING, LIST_FAILED,
    list_failure, owners,
};

fn requested_limit(raw: Option<&str>, cursor: &str, owners: usize) -> Result<Option<i64>, V2Error> {
    match raw {
        Some(text) => Ok(Some(bounded(Some(text), DEFAULT_RECORD_LIMIT, BAD_LIMIT)?)),
        None if owners == 0 || cursor.is_empty() => Ok(Some(DEFAULT_RECORD_LIMIT)),
        None => Ok(None),
    }
}

impl super::TournamentService {
    pub async fn list_records(
        &self,
        state: &ApiState,
        _auth: &Authed,
        id: &str,
        req: RecordsQuery,
    ) -> Result<Json<RecordListResponse>, V2Error> {
        if id.is_empty() {
            return Err(V2Error::invalid_argument(ID_MISSING));
        }
        let owners = owners(req.owner_ids.as_ref(), BAD_OWNERS)?;
        let limit = requested_limit(req.limit.as_deref(), &req.cursor, owners.len())?;
        let override_expiry = number(req.expiry.as_deref(), BAD_EXPIRY)?;

        let queries = q(state).await?;
        let board = load_tournament(&queries, id).await?;
        let end = board.end_time.timestamp();
        if override_expiry == 0 && end > 0 && end <= Utc::now().timestamp() {
            return Err(V2Error::not_found(ENDED));
        }

        let listed = listed_records(
            &queries,
            &board,
            override_expiry,
            limit,
            &req.cursor,
            &owners,
        )
        .await
        .map_err(|error| list_failure(error, LIST_FAILED))?;
        Ok(Json(listed))
    }
}

async fn listed_records(
    queries: &Queries,
    board: &Leaderboard,
    override_expiry: i64,
    limit: Option<i64>,
    cursor: &str,
    owners: &[Uuid],
) -> Result<RecordListResponse, V2Error> {
    let Some(window) = query_window(board, override_expiry)? else {
        return Ok(RecordListResponse::default());
    };
    records_list(queries, board, window, limit, cursor, owners).await
}
