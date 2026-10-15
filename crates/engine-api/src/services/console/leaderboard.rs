//! `/v2/console/leaderboard` and `/v2/console/tournament` — the boards and their
//! records. Both read the same table; a tournament is the `category = 1` rows.

use axum::Json;

use crate::dto::console::Page;
use crate::dto::console::leaderboard::{LeaderboardRecordRow, LeaderboardRow};
use crate::error::V2Error;
use crate::ApiState;

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
            .list_leaderboards_by_category(category, page.limit(), page.offset())
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
}
