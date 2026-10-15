//! `GET /v2/tournament/{tournament_id}/owner/{owner_id}` — one entrant's standing and
//! the neighbours around it, inside the round they are in.
//!
//! Every failure past the board lookup is the same internal error here, as in BaaS.

use axum::Json;
use uuid::Uuid;

use crate::utils::tournament::types::{
    BAD_EXPIRY, BAD_ID, BAD_LIMIT, BAD_OWNER, DEFAULT_PAGE_LIMIT, HAYSTACK_FAILED, NO_OWNER, limit,
};
use super::load_tournament;
use crate::dto::tournament::RecordsQuery;
use crate::repositories::db::Queries;
use crate::repositories::models::Leaderboard;
use crate::services::auth::q;
use crate::error::V2Error;
use crate::services::leaderboard::{haystack, records_list};
use crate::dto::leaderboard::RecordListResponse;
use crate::utils::leaderboard::{number, query_window};
use crate::app::middleware::Authed;
use crate::ApiState;

impl super::TournamentService {
    pub async fn list_records_around_owner(
        &self,
        state: &ApiState,
        _auth: &Authed,
        id: &str,
        owner_id: &str,
        req: RecordsQuery,
    ) -> Result<Json<RecordListResponse>, V2Error> {
        if id.is_empty() {
            return Err(V2Error::invalid_argument(BAD_ID));
        }
        let page_limit = limit(req.limit.as_deref(), DEFAULT_PAGE_LIMIT, BAD_LIMIT)?;
        if owner_id.is_empty() {
            return Err(V2Error::invalid_argument(NO_OWNER));
        }
        let owner = Uuid::parse_str(owner_id).map_err(|_| V2Error::invalid_argument(BAD_OWNER))?;
        let override_expiry = number(req.expiry.as_deref(), BAD_EXPIRY)?;

        let queries = q(state)?;
        let board = load_tournament(&queries, id).await?;
        let listed = standing(&queries, &board, override_expiry, &req.cursor, owner, page_limit)
            .await
            .map_err(|_| V2Error::internal(HAYSTACK_FAILED))?;
        Ok(Json(listed))
    }
}

/// The round's haystack, or the ordinary page a handed-back cursor asks for.
async fn standing(
    queries: &Queries,
    board: &Leaderboard,
    override_expiry: i64,
    cursor: &str,
    owner: Uuid,
    page_limit: i64,
) -> Result<RecordListResponse, V2Error> {
    let Some(window) = query_window(board, override_expiry)? else {
        return Ok(RecordListResponse::default());
    };
    if !cursor.is_empty() {
        return records_list(queries, board, window, Some(page_limit), cursor, &[]).await;
    }
    haystack(queries, board, window, owner, page_limit).await
}
