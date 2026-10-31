use axum::Json;

use super::load_board;
use crate::ApiState;
use crate::app::middleware::Authed;
use crate::dto::Empty;
use crate::error::V2Error;
use crate::repositories::leaderboard::DeleteLeaderboardRecordParams;
use crate::services::deps::q;
use crate::utils::leaderboard::types::{
    AUTH_DELETE, DELETE_FAILED, INVALID_ID, NOT_FOUND, is_tournament,
};
use crate::utils::leaderboard::window::record_window;

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
        let queries = q(state).await?;
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
                tracing::error!("[v2] leaderboard record delete: {error}");
                V2Error::internal(DELETE_FAILED)
            })?;
        Ok(Json(Empty {}))
    }
}
