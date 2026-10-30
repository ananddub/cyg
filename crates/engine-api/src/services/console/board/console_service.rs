use crate::services::deps::q;

use super::*;

impl super::super::ConsoleService {
    pub async fn create_tournament(
        &self,
        state: &ApiState,
        claims: &ConsoleAuthed,
        game: Option<Uuid>,
        body: TournamentWriteRequest,
    ) -> Result<Json<TournamentResponse>, V2Error> {
        claims.require_write()?;
        if body.id.trim().is_empty() {
            return Err(V2Error::invalid_argument(ID_MISSING));
        }
        let id = body.id.trim().to_owned();
        let write = BoardWrite::resolve(&body, BoardWrite::defaults(), id)?;

        let row = definition::tournament_row(&write, game, Utc::now());
        let board = q(state)
            .await?
            .create("leaderboard", Key::String(write.id.clone()), &row)
            .await
            .map_err(|e| {
                if V2Error::is_any_unique_violation(&e) {
                    V2Error::already_exists(ID_TAKEN)
                } else {
                    V2Error::from(e)
                }
            })?;
        audit_board(
            state,
            claims,
            "tournament.create",
            &board.id,
            "Created a tournament.",
        )
        .await;
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

        let board = q(state)
            .await?
            .update_tournament(&crate::repositories::leaderboard::UpdateTournamentParams {
                id: write.id.clone(),
                title: write.title.clone(),
                description: write.description.clone(),
                operator: write.operator,
                sort_order: write.sort_order,
                reset_schedule: write.reset_schedule.clone(),
                metadata: write.metadata.clone(),
                authoritative: write.authoritative,
                duration: write.duration,
                start_time: write.start_time,
                end_time: write.end_time,
                join_required: write.join_required,
                max_size: write.max_size,
                max_num_score: write.max_num_score,
            })
            .await
            .map_err(V2Error::from)?;
        audit_board(
            state,
            claims,
            "tournament.update",
            id,
            "Updated a tournament.",
        )
        .await;
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
        audit_board(
            state,
            claims,
            "tournament.delete",
            id,
            "Deleted a tournament.",
        )
        .await;
        Ok(Json(Empty {}))
    }

    pub async fn delete_leaderboard(
        &self,
        state: &ApiState,
        claims: &ConsoleAuthed,
        id: &str,
    ) -> Result<Json<Empty>, V2Error> {
        claims.require_write()?;
        let current = q(state)
            .await?
            .get_leaderboard(id)
            .await
            .map_err(missing_board)?;
        if is_tournament(&current) {
            return Err(V2Error::not_found(BOARD_NOT_FOUND));
        }
        remove_board(state, id).await?;
        audit_board(
            state,
            claims,
            "leaderboard.delete",
            id,
            "Deleted a leaderboard.",
        )
        .await;
        Ok(Json(Empty {}))
    }
}
async fn tournament_row(state: &ApiState, id: &str) -> Result<Leaderboard, V2Error> {
    let current = q(state)
        .await?
        .get_leaderboard(id)
        .await
        .map_err(|e| match e {
            DbError::NotFound => V2Error::not_found(NOT_FOUND),
            other => V2Error::from(other),
        })?;
    if current.category != 1 {
        return Err(V2Error::not_found(NOT_FOUND));
    }
    Ok(current)
}
fn missing_board(e: DbError) -> V2Error {
    match e {
        DbError::NotFound => V2Error::not_found(BOARD_NOT_FOUND),
        other => V2Error::from(other),
    }
}
pub(super) fn answer(board: &Leaderboard) -> Result<Json<TournamentResponse>, V2Error> {
    Ok(Json(TournamentResponse::from_board(board, Utc::now())?))
}
