use axum::Json;
use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::ApiState;
use crate::app::middleware::Authed;
use crate::db::DbError;
use crate::dto::Empty;
use crate::error::V2Error;
use crate::repositories::db::Queries;
use crate::repositories::models::{Leaderboard, LeaderboardRecord};
use crate::services::deps::q;
use crate::utils::leaderboard::tournament_window;
use crate::utils::tournament::types::{BAD_ID, DELETE_FAILED};

impl super::TournamentService {
    pub async fn delete_record(
        &self,
        state: &ApiState,
        auth: &Authed,
        id: &str,
    ) -> Result<Json<Empty>, V2Error> {
        if id.is_empty() {
            return Err(V2Error::invalid_argument(BAD_ID));
        }
        let queries = q(state).await?;
        let board = load(&queries, id).await?;
        if board.authoritative {
            return Err(V2Error::internal(DELETE_FAILED));
        }

        let expiry = tournament_window(&board)?;
        remove(&queries, &board, auth.uid(), expiry).await?;
        Ok(Json(Empty {}))
    }
}

async fn load(queries: &Queries, id: &str) -> Result<Leaderboard, V2Error> {
    queries.get_tournament(id).await.map_err(|error| {
        tracing::error!("[v2] tournament delete load: {error}");
        V2Error::internal(DELETE_FAILED)
    })
}

async fn remove(
    queries: &Queries,
    board: &Leaderboard,
    owner: Uuid,
    expiry: DateTime<Utc>,
) -> Result<(), V2Error> {
    let deleted: Vec<LeaderboardRecord> = queries
        .delete_tournament_entry(&board.id, owner, expiry)
        .await
        .map_err(delete_failure)?;
    if board.max_size > 0 && !deleted.is_empty() {
        queries
            .shrink_tournament_size(&board.id)
            .await
            .map_err(delete_failure)?;
    }
    Ok(())
}

fn delete_failure(error: DbError) -> V2Error {
    tracing::error!("[v2] tournament delete: {error}");
    V2Error::internal(DELETE_FAILED)
}
