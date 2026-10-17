//! Auto-registered bindings for `/v2/console/leaderboard*` and
//! `/v2/console/tournament`. Two controllers because `auto_route` gives one base
//! path per impl.

use std::sync::Arc;

use auto_route::controller;
use axum::extract::Path;
use axum::response::{IntoResponse, Response};

use crate::app::middleware::ConsoleAuthed;
use crate::app::params::{Body, Params};
use crate::current_state;
use crate::dto::console::Page;
use crate::dto::console::leaderboard::TournamentWriteRequest;
use crate::services::console::ConsoleService;

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
    async fn get(&self, _auth: ConsoleAuthed, Path(id): Path<String>) -> Response {
        let state = current_state();
        self.service.get_leaderboard(&state, &id).await.into_response()
    }

    #[get("/{id}/records")]
    async fn records(&self, _auth: ConsoleAuthed, Path(id): Path<String>) -> Response {
        let state = current_state();
        self.service
            .list_leaderboard_records(&state, &id)
            .await
            .into_response()
    }

    #[delete("/{id}")]
    async fn delete(&self, auth: ConsoleAuthed, Path(id): Path<String>) -> Response {
        let state = current_state();
        self.service
            .delete_leaderboard(&state, &auth, &id)
            .await
            .into_response()
    }

    /// Remove one owner's record from a plain leaderboard.
    #[delete("/{id}/owner/{owner_id}")]
    async fn delete_record(
        &self,
        auth: ConsoleAuthed,
        Path((id, owner_id)): Path<(String, String)>,
    ) -> Response {
        let state = current_state();
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
        self.service.list_tournaments(&state, page).await.into_response()
    }

    #[post("")]
    async fn create(
        &self,
        auth: ConsoleAuthed,
        Body(body): Body<TournamentWriteRequest>,
    ) -> Response {
        let state = current_state();
        self.service
            .create_tournament(&state, &auth, body)
            .await
            .into_response()
    }

    #[put("/{id}")]
    async fn update(
        &self,
        auth: ConsoleAuthed,
        Path(id): Path<String>,
        Body(body): Body<TournamentWriteRequest>,
    ) -> Response {
        let state = current_state();
        self.service
            .update_tournament(&state, &auth, &id, body)
            .await
            .into_response()
    }

    #[delete("/{id}")]
    async fn delete(&self, auth: ConsoleAuthed, Path(id): Path<String>) -> Response {
        let state = current_state();
        self.service
            .delete_tournament(&state, &auth, &id)
            .await
            .into_response()
    }
}
