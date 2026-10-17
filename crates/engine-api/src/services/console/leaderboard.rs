//! `/v2/console/leaderboard` and `/v2/console/tournament` — the boards and their
//! records. Both read the same table; a tournament is the `category = 1` rows.

use axum::Json;

use crate::app::middleware::ConsoleAuthed;
use crate::dto::Empty;
use crate::dto::console::Page;
use crate::dto::console::leaderboard::{LeaderboardRecordRow, LeaderboardRow};
use crate::error::V2Error;
use crate::repositories::console::ListLeaderboardsByCategoryParams;
use crate::repositories::leaderboard::DeleteLeaderboardRecordParams;
use crate::utils::leaderboard::types::is_tournament;
use crate::utils::leaderboard::window::record_window;
use crate::ApiState;
use uuid::Uuid;

/// A board's page of records; the player API caps a listing at 100 the same way.
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
        let rows = super::queries(state)?
            .list_leaderboards_by_category(&ListLeaderboardsByCategoryParams {
                category,
                limit: page.limit(),
                offset: page.offset(),
            })
            .await?;
        Ok(Json(rows.into_iter().map(LeaderboardRow::from).collect()))
    }

    pub async fn get_leaderboard(
        &self,
        state: &ApiState,
        id: &str,
    ) -> Result<Json<LeaderboardRow>, V2Error> {
        let board = super::queries(state)?.get_leaderboard(id).await?;
        Ok(Json(LeaderboardRow::from(board)))
    }

    pub async fn list_leaderboard_records(
        &self,
        state: &ApiState,
        id: &str,
    ) -> Result<Json<Vec<LeaderboardRecordRow>>, V2Error> {
        let rows = super::queries(state)?
            .list_all_leaderboard_records(id, MAX_RECORDS)
            .await?;
        Ok(Json(rows.into_iter().map(LeaderboardRecordRow::from).collect()))
    }

    /// Delete one owner's record from a plain leaderboard, the way the console's
    /// record-delete handler does: it addresses a named owner and bypasses the
    /// authoritative check an operator is exempt from. The window the record sits in
    /// is the board's current one, the same the player route deletes, so an operator
    /// removes the score a player would remove for themselves. A board with rounds is
    /// not a leaderboard here: the tournament route owns those records.
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
        let queries = super::queries(state)?;
        let board = queries
            .get_leaderboard(id)
            .await
            .map_err(|e| match e {
                sqlx::Error::RowNotFound => V2Error::not_found("Leaderboard not found."),
                other => V2Error::from_sqlx(other),
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
                eprintln!("[console] leaderboard record delete: {error}");
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
