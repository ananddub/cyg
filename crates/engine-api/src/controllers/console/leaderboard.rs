use std::sync::Arc;

use auto_route::controller;
use axum::extract::Path;
use axum::response::{IntoResponse, Response};

use crate::app::middleware::ConsoleAuthed;
use crate::app::params::{Body, Params};
use crate::current_state;
use crate::dto::console::leaderboard::TournamentWriteRequest;
use crate::dto::console::{GameScope, Page};
use crate::error::V2Error;
use crate::services::console::ConsoleService;
use crate::services::games::tenant;

struct ConsoleLeaderboardController {
    service: Arc<ConsoleService>,
}

#[controller("/v2/console/leaderboard")]
impl ConsoleLeaderboardController {
    fn new(service: Arc<ConsoleService>) -> Self {
        Self { service }
    }

    #[get("")]
    async fn list(&self, _auth: ConsoleAuthed, Params(page): Params<Page>) -> Response {
        let state = current_state();
        self.service
            .list_leaderboards(&state, page)
            .await
            .into_response()
    }

    #[get("/{id}")]
    async fn get(
        &self,
        _auth: ConsoleAuthed,
        Path(id): Path<String>,
        Params(scope): Params<GameScope>,
    ) -> Response {
        let state = current_state();
        if let Err(err) = guard_board(&state, scope.id(), &id).await {
            return err.into_response();
        }
        self.service
            .get_leaderboard(&state, &id)
            .await
            .into_response()
    }

    #[get("/{id}/records")]
    async fn records(
        &self,
        _auth: ConsoleAuthed,
        Path(id): Path<String>,
        Params(scope): Params<GameScope>,
    ) -> Response {
        let state = current_state();
        if let Err(err) = guard_board(&state, scope.id(), &id).await {
            return err.into_response();
        }
        self.service
            .list_leaderboard_records(&state, &id)
            .await
            .into_response()
    }

    #[delete("/{id}")]
    async fn delete(
        &self,
        auth: ConsoleAuthed,
        Path(id): Path<String>,
        Params(scope): Params<GameScope>,
    ) -> Response {
        let state = current_state();
        if let Err(err) = guard_board(&state, scope.id(), &id).await {
            return err.into_response();
        }
        self.service
            .delete_leaderboard(&state, &auth, &id)
            .await
            .into_response()
    }

    #[delete("/{id}/owner/{owner_id}")]
    async fn delete_record(
        &self,
        auth: ConsoleAuthed,
        Path((id, owner_id)): Path<(String, String)>,
        Params(scope): Params<GameScope>,
    ) -> Response {
        let state = current_state();
        if let Err(err) = guard_board(&state, scope.id(), &id).await {
            return err.into_response();
        }
        self.service
            .delete_leaderboard_record(&state, &auth, &id, &owner_id)
            .await
            .into_response()
    }
}

struct ConsoleTournamentController {
    service: Arc<ConsoleService>,
}

#[controller("/v2/console/tournament")]
impl ConsoleTournamentController {
    fn new(service: Arc<ConsoleService>) -> Self {
        Self { service }
    }

    #[get("")]
    async fn list(&self, _auth: ConsoleAuthed, Params(page): Params<Page>) -> Response {
        let state = current_state();
        self.service
            .list_tournaments(&state, page)
            .await
            .into_response()
    }

    #[post("")]
    async fn create(
        &self,
        auth: ConsoleAuthed,
        Params(scope): Params<GameScope>,
        Body(body): Body<TournamentWriteRequest>,
    ) -> Response {
        let state = current_state();
        self.service
            .create_tournament(&state, &auth, scope.id(), body)
            .await
            .into_response()
    }

    #[put("/{id}")]
    async fn update(
        &self,
        auth: ConsoleAuthed,
        Path(id): Path<String>,
        Params(scope): Params<GameScope>,
        Body(body): Body<TournamentWriteRequest>,
    ) -> Response {
        let state = current_state();
        if let Err(err) = guard_tournament(&state, scope.id(), &id).await {
            return err.into_response();
        }
        self.service
            .update_tournament(&state, &auth, &id, body)
            .await
            .into_response()
    }

    #[delete("/{id}")]
    async fn delete(
        &self,
        auth: ConsoleAuthed,
        Path(id): Path<String>,
        Params(scope): Params<GameScope>,
    ) -> Response {
        let state = current_state();
        if let Err(err) = guard_tournament(&state, scope.id(), &id).await {
            return err.into_response();
        }
        self.service
            .delete_tournament(&state, &auth, &id)
            .await
            .into_response()
    }
}

async fn guard_board(
    state: &crate::ApiState,
    game: Option<uuid::Uuid>,
    id: &str,
) -> Result<(), V2Error> {
    if tenant::board_in_game(state, game, id).await? {
        Ok(())
    } else {
        Err(V2Error::not_found("Leaderboard not found."))
    }
}

async fn guard_tournament(
    state: &crate::ApiState,
    game: Option<uuid::Uuid>,
    id: &str,
) -> Result<(), V2Error> {
    if tenant::board_in_game(state, game, id).await? {
        Ok(())
    } else {
        Err(V2Error::not_found("Tournament not found."))
    }
}
