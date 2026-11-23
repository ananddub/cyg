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

type LeaderboardRowList = Vec<crate::dto::console::leaderboard::LeaderboardRow>;
type LeaderboardRecordRowList = Vec<crate::dto::console::leaderboard::LeaderboardRecordRow>;
#[controller("/v1/console/leaderboard")]
impl ConsoleLeaderboardController {
    fn new(service: Arc<ConsoleService>) -> Self {
        Self { service }
    }

    #[get("", response_body = LeaderboardRowList)]
    async fn list(&self, _auth: ConsoleAuthed, Params(page): Params<Page>) -> Response {
        let state = current_state();
        self.service.list_leaderboards(page).await.into_response()
    }

    #[get("/{id}", response_body = crate::dto::console::leaderboard::LeaderboardRow)]
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
        self.service.get_leaderboard(&id).await.into_response()
    }

    #[get("/{id}/records", response_body = LeaderboardRecordRowList)]
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
            .list_leaderboard_records(&id)
            .await
            .into_response()
    }

    #[delete("/{id}", response_body = crate::dto::Empty)]
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
            .delete_leaderboard(&auth, &id)
            .await
            .into_response()
    }

    #[delete("/{id}/owner/{owner_id}", response_body = crate::dto::Empty)]
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
            .delete_leaderboard_record(&auth, &id, &owner_id)
            .await
            .into_response()
    }
}

struct ConsoleTournamentController {
    service: Arc<ConsoleService>,
}

#[controller("/v1/console/tournament")]
impl ConsoleTournamentController {
    fn new(service: Arc<ConsoleService>) -> Self {
        Self { service }
    }

    #[get("", response_body = LeaderboardRowList)]
    async fn list(&self, _auth: ConsoleAuthed, Params(page): Params<Page>) -> Response {
        let state = current_state();
        self.service.list_tournaments(page).await.into_response()
    }

    #[post("", response_body = crate::dto::tournament::TournamentResponse)]
    async fn create(
        &self,
        auth: ConsoleAuthed,
        Params(scope): Params<GameScope>,
        Body(body): Body<TournamentWriteRequest>,
    ) -> Response {
        let state = current_state();
        self.service
            .create_tournament(&auth, scope.id(), body)
            .await
            .into_response()
    }

    #[put("/{id}", response_body = crate::dto::tournament::TournamentResponse)]
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
            .update_tournament(&auth, &id, body)
            .await
            .into_response()
    }

    #[delete("/{id}", response_body = crate::dto::Empty)]
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
            .delete_tournament(&auth, &id)
            .await
            .into_response()
    }
}

async fn guard_board(
    state: &crate::ApiState,
    game: Option<uuid::Uuid>,
    id: &str,
) -> Result<(), V2Error> {
    if tenant::board_in_game(&*state.plane(), game, id).await? {
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
    if tenant::board_in_game(&*state.plane(), game, id).await? {
        Ok(())
    } else {
        Err(V2Error::not_found("Tournament not found."))
    }
}
