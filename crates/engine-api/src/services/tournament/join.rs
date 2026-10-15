//! `POST /v2/tournament/{tournament_id}/join` — take a place in the round.
//!
//! A join is a zero-score row in the current window, and only boards that ask for one get it:
//! BaaS answers a join to a board without `join_required` as a success that wrote nothing.

use axum::Json;
use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

use crate::dto::Empty;
use crate::utils::tournament::statement::{JOIN_SIZE_SQL, JOIN_SQL};
use crate::utils::tournament::types::{INACTIVE_JOIN, JOIN_FAILED, MAX_SIZE_JOIN};
use super::load_tournament;
use crate::repositories::models::Leaderboard;
use crate::services::auth::q;
use crate::error::V2Error;
use crate::services::leaderboard::pool;
use crate::utils::leaderboard::{at, calculate};
use crate::app::middleware::Authed;
use crate::ApiState;

impl super::TournamentService {
    pub async fn join_tournament(
        &self,
        state: &ApiState,
        auth: &Authed,
        id: &str,
    ) -> Result<Json<Empty>, V2Error> {
        let queries = q(state)?;
        let board = load_tournament(&queries, id).await?;
        if !board.join_required {
            return Ok(Json(Empty {}));
        }

        let now = Utc::now();
        let deadlines = calculate(&board, now)?;
        if deadlines.end_active <= now.timestamp() {
            return Err(V2Error::invalid_argument(INACTIVE_JOIN));
        }

        enter(
            pool(state)?,
            &board,
            auth.uid(),
            auth.0.un.as_str(),
            at(deadlines.expiry),
        )
        .await?;
        Ok(Json(Empty {}))
    }
}

/// The join, and the counter it moves. A row that was already there is a no-op
/// the caller is not told about, and a board that filled up rolls back.
async fn enter(
    db: &PgPool,
    board: &Leaderboard,
    owner: Uuid,
    username: &str,
    expiry: DateTime<Utc>,
) -> Result<(), V2Error> {
    let mut tx = db.begin().await.map_err(failure)?;

    let inserted = sqlx::query(JOIN_SQL)
        .bind(&board.id)
        .bind(owner)
        .bind(expiry)
        .bind(username)
        .bind(board.max_num_score)
        .execute(&mut *tx)
        .await
        .map_err(failure)?
        .rows_affected();
    if inserted == 0 {
        return commit(tx).await;
    }

    let room = sqlx::query(JOIN_SIZE_SQL)
        .bind(&board.id)
        .execute(&mut *tx)
        .await
        .map_err(failure)?
        .rows_affected();
    if room == 0 {
        return Err(V2Error::invalid_argument(MAX_SIZE_JOIN));
    }
    commit(tx).await
}

fn failure(error: sqlx::Error) -> V2Error {
    eprintln!("[v2] tournament join: {error}");
    V2Error::internal(JOIN_FAILED)
}

async fn commit(tx: sqlx::Transaction<'_, sqlx::Postgres>) -> Result<(), V2Error> {
    tx.commit().await.map_err(failure)
}
