//! `DELETE /v2/tournament/{tournament_id}` — the caller's record for the round.
//!
//! The tournament core reports failures with the tournament's own error values while this
//! route's switch compares against the leaderboard's, so nearly every failure is internal.

use axum::Json;
use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

use crate::dto::Empty;
use crate::utils::tournament::statement::{DELETE_SQL, RELEASE_SIZE_SQL};
use crate::utils::tournament::types::{BAD_ID, DELETE_FAILED};
use crate::repositories::db::Queries;
use crate::repositories::models::Leaderboard;
use crate::services::auth::q;
use crate::error::V2Error;
use crate::services::leaderboard::pool;
use crate::utils::leaderboard::tournament_window;
use crate::app::middleware::Authed;
use crate::ApiState;

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
        let queries = q(state)?;
        let board = load(&queries, id).await?;
        if board.authoritative {
            return Err(V2Error::internal(DELETE_FAILED));
        }

        let expiry = tournament_window(&board)?;
        remove(pool(state)?, &board, auth.uid(), expiry).await?;
        Ok(Json(Empty {}))
    }
}

/// A board that is absent, or is not a tournament, is the same internal error
/// here as a database failure; only an unmatched error value says otherwise.
async fn load(queries: &Queries, id: &str) -> Result<Leaderboard, V2Error> {
    queries
        .get_tournament(id)
        .await
        .map_err(|error| {
            eprintln!("[v2] tournament delete load: {error}");
            V2Error::internal(DELETE_FAILED)
        })
}

async fn remove(
    db: &PgPool,
    board: &Leaderboard,
    owner: Uuid,
    expiry: DateTime<Utc>,
) -> Result<(), V2Error> {
    let mut tx = db.begin().await.map_err(delete_failure)?;

    let deleted = sqlx::query(DELETE_SQL)
        .bind(&board.id)
        .bind(owner)
        .bind(expiry)
        .execute(&mut *tx)
        .await
        .map_err(delete_failure)?
        .rows_affected();
    if board.max_size > 0 && deleted > 0 {
        sqlx::query(RELEASE_SIZE_SQL)
            .bind(&board.id)
            .execute(&mut *tx)
            .await
            .map_err(delete_failure)?;
    }

    tx.commit().await.map_err(delete_failure)
}

fn delete_failure(error: sqlx::Error) -> V2Error {
    eprintln!("[v2] tournament delete: {error}");
    V2Error::internal(DELETE_FAILED)
}
