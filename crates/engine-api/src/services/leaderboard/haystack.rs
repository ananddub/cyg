//! `GET /v2/leaderboard/{leaderboard_id}/owner/{owner_id}` — the owner's record with
//! its neighbours on either side, so a client renders a standing without walking the
//! board. Handing a cursor back turns the request into an ordinary page of the same
//! listing, which is what `records_list` answers.

use axum::Json;
use uuid::Uuid;

use super::list::records_list;
use super::load_board;
use super::neighbours::haystack;
use crate::ApiState;
use crate::app::middleware::Authed;
use crate::dto::leaderboard::{HaystackQuery, RecordListResponse};
use crate::error::V2Error;
use crate::services::auth::q;
use crate::utils::leaderboard::types::{
    BAD_EXPIRY, BAD_OWNER, INVALID_ID, MAX_OWNER_LIMIT, NO_OWNER, OWNER_LIMIT, number,
};
use crate::utils::leaderboard::window::query_window;

impl super::LeaderboardService {
    pub async fn list_records_around_owner(
        &self,
        state: &ApiState,
        _auth: &Authed,
        id: &str,
        owner_id: &str,
        req: HaystackQuery,
    ) -> Result<Json<RecordListResponse>, V2Error> {
        if id.is_empty() {
            return Err(V2Error::invalid_argument(INVALID_ID));
        }
        let limit = match req.limit.as_deref() {
            Some(raw) => {
                let value = number(Some(raw), OWNER_LIMIT)?;
                if (1..=MAX_OWNER_LIMIT).contains(&value) {
                    value
                } else {
                    return Err(V2Error::invalid_argument(OWNER_LIMIT));
                }
            }
            None => 1,
        };
        if owner_id.is_empty() {
            return Err(V2Error::invalid_argument(NO_OWNER));
        }
        let owner = Uuid::parse_str(owner_id).map_err(|_| V2Error::invalid_argument(BAD_OWNER))?;
        let override_expiry = number(req.expiry.as_deref(), BAD_EXPIRY)?;

        let queries = q(state)?;
        let board = load_board(&queries, id).await?;
        let Some(window) = query_window(&board, override_expiry)? else {
            return Ok(Json(RecordListResponse::default()));
        };
        if !req.cursor.is_empty() {
            let listed =
                records_list(&queries, &board, window, Some(limit), &req.cursor, &[]).await?;
            return Ok(Json(listed));
        }
        Ok(Json(
            haystack(&queries, &board, window, owner, limit).await?,
        ))
    }
}
