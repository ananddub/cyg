//! `DELETE /v2/leaderboard/{leaderboard_id}` — the caller's own record for the
//! window the board's reset schedule names. A tournament board is not a
//! leaderboard here: BaaS answers not-found, so the client reaches for the
//! tournament route instead of deleting a round's record by accident.

use axum::Json;

use crate::dto::Empty;
use crate::utils::leaderboard::types::{AUTH_DELETE, DELETE_FAILED, INVALID_ID, NOT_FOUND, is_tournament};
use crate::utils::leaderboard::window::record_window;
use super::load_board;
use crate::repositories::leaderboard::DeleteLeaderboardRecordParams;
use crate::services::auth::q;
use crate::error::V2Error;
use crate::app::middleware::Authed;
use crate::ApiState;

impl super::LeaderboardService {
    pub async fn delete_record(
        &self,
        state: &ApiState,
        auth: &Authed,
        id: &str,
    ) -> Result<Json<Empty>, V2Error> {
        if id.is_empty() {
            return Err(V2Error::invalid_argument(INVALID_ID));
        }
        let queries = q(state)?;
        let board = load_board(&queries, id).await?;
        if is_tournament(&board) {
            return Err(V2Error::not_found(NOT_FOUND));
        }
        if board.authoritative {
            return Err(V2Error::permission_denied(AUTH_DELETE));
        }
        let expiry_time = record_window(&board)?;
        queries
            .delete_leaderboard_record(&DeleteLeaderboardRecordParams {
                leaderboard_id: board.id,
                owner_id: auth.uid(),
                expiry_time,
            })
            .await
            .map_err(|error| {
                eprintln!("[v2] leaderboard record delete: {error}");
                V2Error::internal(DELETE_FAILED)
            })?;
        Ok(Json(Empty {}))
    }
}
