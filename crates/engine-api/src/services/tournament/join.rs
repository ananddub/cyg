use axum::Json;
use chrono::{DateTime, Utc};
use uuid::Uuid;

use super::load_tournament;
use crate::ApiState;
use crate::app::middleware::Authed;
use crate::db::DbError;
use crate::dto::Empty;
use crate::error::V2Error;
use crate::repositories::db::{Queries, params, ts};
use crate::repositories::models::{Leaderboard, LeaderboardRecord};
use crate::services::deps::q;
use crate::utils::leaderboard::{at, calculate};
use crate::utils::tournament::statement::{
    EXISTS_SQL, JOIN_SIZE_SQL, RELEASE_SIZE_SQL, joined_row,
};
use crate::utils::tournament::types::{INACTIVE_JOIN, JOIN_FAILED, MAX_SIZE_JOIN};

impl super::TournamentService {
    pub async fn join_tournament(
        &self,
        state: &ApiState,
        auth: &Authed,
        id: &str,
    ) -> Result<Json<Empty>, V2Error> {
        let queries = q(state).await?;
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
            &queries,
            &board,
            auth.uid(),
            auth.0.un.as_str(),
            at(deadlines.expiry),
        )
        .await?;
        Ok(Json(Empty {}))
    }
}

async fn enter(
    queries: &Queries,
    board: &Leaderboard,
    owner: Uuid,
    username: &str,
    expiry: DateTime<Utc>,
) -> Result<(), V2Error> {
    let existing: Option<LeaderboardRecord> = queries
        .opt(
            EXISTS_SQL,
            params!(
                leaderboard_id = &board.id,
                owner_id = owner.to_string(),
                expiry_time = ts(expiry),
            ),
        )
        .await
        .map_err(failure)?;
    if existing.is_some() {
        return Ok(());
    }

    let room: Option<Leaderboard> = queries
        .opt(JOIN_SIZE_SQL, params!(leaderboard_id = &board.id))
        .await
        .map_err(failure)?;
    if room.is_none() {
        return Err(V2Error::invalid_argument(MAX_SIZE_JOIN));
    }

    let row = joined_row(
        &board.id,
        owner,
        username,
        expiry,
        board.max_num_score,
        Utc::now(),
    );
    match queries.create_leaderboard_record(&row).await {
        Ok(_) => Ok(()),
        Err(DbError::Unique(_)) => {
            release(queries, board).await;
            Ok(())
        }
        Err(error) => {
            release(queries, board).await;
            Err(failure(error))
        }
    }
}

async fn release(queries: &Queries, board: &Leaderboard) {
    if let Err(error) = queries
        .exec(RELEASE_SIZE_SQL, params!(leaderboard_id = &board.id))
        .await
    {
        tracing::error!("[v2] tournament join: releasing a place: {error}");
    }
}

fn failure(error: DbError) -> V2Error {
    tracing::error!("[v2] tournament join: {error}");
    V2Error::internal(JOIN_FAILED)
}
