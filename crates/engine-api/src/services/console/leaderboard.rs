use axum::Json;

use crate::ApiState;
use crate::app::middleware::ConsoleAuthed;
use crate::dto::Empty;
use crate::dto::console::Page;
use crate::dto::console::leaderboard::{LeaderboardRecordRow, LeaderboardRow};
use crate::error::V2Error;
use crate::services::deps::q;
use crate::repositories::leaderboard::DeleteLeaderboardRecordParams;
use crate::utils::leaderboard::types::is_tournament;
use crate::utils::leaderboard::window::record_window;
use uuid::Uuid;

const MAX_RECORDS: i64 = 100;

impl super::ConsoleService {
    pub async fn list_leaderboards(
        &self,
        state: &ApiState,
        page: Page,
    ) -> Result<Json<Vec<LeaderboardRow>>, V2Error> {
        self.boards(state, page, 0).await
    }

    pub async fn list_tournaments(
        &self,
        state: &ApiState,
        page: Page,
    ) -> Result<Json<Vec<LeaderboardRow>>, V2Error> {
        self.boards(state, page, 1).await
    }

    async fn boards(
        &self,
        state: &ApiState,
        page: Page,
        category: i16,
    ) -> Result<Json<Vec<LeaderboardRow>>, V2Error> {
        let rows = crate::services::games::tenant::list_boards(
            state,
            page.game_id,
            category,
            page.limit(),
            page.offset(),
        )
        .await?;
        Ok(Json(rows.into_iter().map(LeaderboardRow::from).collect()))
    }

    pub async fn get_leaderboard(
        &self,
        state: &ApiState,
        id: &str,
    ) -> Result<Json<LeaderboardRow>, V2Error> {
        let board = q(state).await?.get_leaderboard(id).await?;
        Ok(Json(LeaderboardRow::from(board)))
    }

    pub async fn list_leaderboard_records(
        &self,
        state: &ApiState,
        id: &str,
    ) -> Result<Json<Vec<LeaderboardRecordRow>>, V2Error> {
        let rows = q(state).await?
            .list_all_leaderboard_records(id, MAX_RECORDS)
            .await?;
        Ok(Json(
            rows.into_iter().map(LeaderboardRecordRow::from).collect(),
        ))
    }

    pub async fn delete_leaderboard_record(
        &self,
        state: &ApiState,
        claims: &ConsoleAuthed,
        id: &str,
        owner_id: &str,
    ) -> Result<Json<Empty>, V2Error> {
        claims.require_write()?;
        if id.is_empty() {
            return Err(V2Error::invalid_argument("Invalid leaderboard ID."));
        }
        let owner = Uuid::parse_str(owner_id)
            .map_err(|_| V2Error::invalid_argument("Invalid owner ID."))?;
        let queries = q(state).await?;
        let board = queries.get_leaderboard(id).await.map_err(|e| match e {
            crate::db::DbError::NotFound => V2Error::not_found("Leaderboard not found."),
            other => V2Error::from(other),
        })?;
        if is_tournament(&board) {
            return Err(V2Error::not_found("Leaderboard not found."));
        }
        let expiry_time = record_window(&board)?;
        queries
            .delete_leaderboard_record(&DeleteLeaderboardRecordParams {
                leaderboard_id: board.id,
                owner_id: owner,
                expiry_time,
            })
            .await
            .map_err(|error| {
                tracing::error!("[console] leaderboard record delete: {error}");
                V2Error::internal("Error deleting score from leaderboard.")
            })?;
        super::audit(
            state,
            claims,
            "leaderboard.record.delete",
            "leaderboard",
            None,
            &format!("Deleted {owner}'s record on leaderboard {id}."),
        )
        .await;
        Ok(Json(Empty {}))
    }
}
