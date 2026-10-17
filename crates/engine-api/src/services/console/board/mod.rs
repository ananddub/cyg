//! `/v2/console/tournament` writes and `/v2/console/leaderboard` delete.
//!
//! The board row is the one table both halves of the surface share, and no
//! generated query defines a tournament's round (`duration`, `start_time`,
//! `end_time`), so these statements are hand-written and run against the pool,
//! the way the storage service's do. A tournament is the `category = 1` row, as
//! the console's listing already counts them; deleting a board takes its records
//! with it through the cascade the schema declares.

mod definition;

use axum::Json;
use chrono::Utc;

use crate::app::middleware::ConsoleAuthed;
use crate::dto::Empty;
use crate::dto::console::leaderboard::TournamentWriteRequest;
use crate::dto::tournament::TournamentResponse;
use crate::error::V2Error;
use crate::repositories::models::Leaderboard;
use crate::utils::leaderboard::types::{NOT_FOUND as BOARD_NOT_FOUND, is_tournament};
use crate::utils::tournament::types::NOT_FOUND;
use crate::ApiState;

use definition::{BoardWrite, ID_MISSING, ID_TAKEN};

impl super::ConsoleService {
    pub async fn create_tournament(
        &self,
        state: &ApiState,
        claims: &ConsoleAuthed,
        body: TournamentWriteRequest,
    ) -> Result<Json<TournamentResponse>, V2Error> {
        claims.require_write()?;
        if body.id.trim().is_empty() {
            return Err(V2Error::invalid_argument(ID_MISSING));
        }
        let id = body.id.trim().to_owned();
        let write = BoardWrite::resolve(&body, BoardWrite::defaults(), id)?;

        let board = sqlx::query_as::<_, Leaderboard>(definition::INSERT_SQL)
            .bind(&write.id)
            .bind(&write.title)
            .bind(&write.description)
            .bind(write.operator)
            .bind(write.sort_order)
            .bind(&write.reset_schedule)
            .bind(&write.metadata)
            .bind(write.authoritative)
            .bind(write.duration)
            .bind(write.start_time)
            .bind(write.end_time)
            .bind(write.join_required)
            .bind(write.max_size)
            .bind(write.max_num_score)
            .fetch_one(super::pool(state)?)
            .await
            .map_err(|e| {
                if V2Error::is_any_unique_violation(&e) {
                    V2Error::already_exists(ID_TAKEN)
                } else {
                    V2Error::from_sqlx(e)
                }
            })?;
        audit_board(state, claims, "tournament.create", &board.id, "Created a tournament.").await;
        answer(&board)
    }

    pub async fn update_tournament(
        &self,
        state: &ApiState,
        claims: &ConsoleAuthed,
        id: &str,
        body: TournamentWriteRequest,
    ) -> Result<Json<TournamentResponse>, V2Error> {
        claims.require_write()?;
        let current = tournament_row(state, id).await?;
        let write = BoardWrite::resolve(&body, BoardWrite::stored(&current), id.to_owned())?;

        let board = sqlx::query_as::<_, Leaderboard>(definition::UPDATE_SQL)
            .bind(&write.id)
            .bind(&write.title)
            .bind(&write.description)
            .bind(write.operator)
            .bind(write.sort_order)
            .bind(&write.reset_schedule)
            .bind(&write.metadata)
            .bind(write.authoritative)
            .bind(write.duration)
            .bind(write.start_time)
            .bind(write.end_time)
            .bind(write.join_required)
            .bind(write.max_size)
            .bind(write.max_num_score)
            .fetch_one(super::pool(state)?)
            .await
            .map_err(V2Error::from_sqlx)?;
        audit_board(state, claims, "tournament.update", id, "Updated a tournament.").await;
        answer(&board)
    }

    pub async fn delete_tournament(
        &self,
        state: &ApiState,
        claims: &ConsoleAuthed,
        id: &str,
    ) -> Result<Json<Empty>, V2Error> {
        claims.require_write()?;
        tournament_row(state, id).await?;
        remove_board(state, id).await?;
        audit_board(state, claims, "tournament.delete", id, "Deleted a tournament.").await;
        Ok(Json(Empty {}))
    }

    /// Delete a plain leaderboard. A board with rounds is not one: the player
    /// route already answers "not found" for a tournament id, and an operator
    /// reaches a tournament through the tournament route instead. The cascade
    /// removes every record the board holds, the same delete the player route
    /// runs one row at a time.
    pub async fn delete_leaderboard(
        &self,
        state: &ApiState,
        claims: &ConsoleAuthed,
        id: &str,
    ) -> Result<Json<Empty>, V2Error> {
        claims.require_write()?;
        let current = super::queries(state)?
            .get_leaderboard(id)
            .await
            .map_err(missing_board)?;
        if is_tournament(&current) {
            return Err(V2Error::not_found(BOARD_NOT_FOUND));
        }
        remove_board(state, id).await?;
        audit_board(state, claims, "leaderboard.delete", id, "Deleted a leaderboard.").await;
        Ok(Json(Empty {}))
    }
}

/// The board a tournament route addresses; a row that is not there — or is not
/// the console's tournament category — answers the route's own not-found.
async fn tournament_row(state: &ApiState, id: &str) -> Result<Leaderboard, V2Error> {
    let current = super::queries(state)?
        .get_leaderboard(id)
        .await
        .map_err(|e| match e {
            sqlx::Error::RowNotFound => V2Error::not_found(NOT_FOUND),
            other => V2Error::from_sqlx(other),
        })?;
    if current.category != 1 {
        return Err(V2Error::not_found(NOT_FOUND));
    }
    Ok(current)
}

fn missing_board(e: sqlx::Error) -> V2Error {
    match e {
        sqlx::Error::RowNotFound => V2Error::not_found(BOARD_NOT_FOUND),
        other => V2Error::from_sqlx(other),
    }
}

/// A created or updated board answers with the player tournament's own shape.
fn answer(board: &Leaderboard) -> Result<Json<TournamentResponse>, V2Error> {
    Ok(Json(TournamentResponse::from_board(board, Utc::now())?))
}

async fn remove_board(state: &ApiState, id: &str) -> Result<(), V2Error> {
    let removed = sqlx::query(definition::DELETE_SQL)
        .bind(id)
        .execute(super::pool(state)?)
        .await
        .map_err(V2Error::from_sqlx)?;
    if removed.rows_affected() == 0 {
        return Err(V2Error::not_found(BOARD_NOT_FOUND));
    }
    Ok(())
}

/// The board's id is a string, so the audit target column — a uuid — stays empty.
async fn audit_board(
    state: &ApiState,
    claims: &ConsoleAuthed,
    action: &str,
    id: &str,
    message: &str,
) {
    super::audit(state, claims, action, "leaderboard", None, &format!("{message} {id}")).await;
}
