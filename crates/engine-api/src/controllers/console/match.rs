use std::sync::Arc;

use auto_route::controller;
use axum::extract::Path;
use axum::response::{IntoResponse, Response};

use crate::app::middleware::ConsoleAuthed;
use crate::app::params::Params;
use crate::current_state;
use crate::dto::console::{GameScope, Page};
use crate::services::console::ConsoleService;

struct ConsoleMatchController {
    service: Arc<ConsoleService>,
}

#[controller("/v2/console/match")]
impl ConsoleMatchController {
    fn new(service: Arc<ConsoleService>) -> Self {
        Self { service }
    }

    #[get("")]
    async fn list(&self, _auth: ConsoleAuthed, Params(page): Params<Page>) -> Response {
        let state = current_state();
        self.service
            .list_matches(&state, page)
            .await
            .into_response()
    }

    #[get("/{id}/state")]
    async fn state(
        &self,
        _auth: ConsoleAuthed,
        Path(id): Path<String>,
        Params(scope): Params<GameScope>,
    ) -> Response {
        let state = current_state();
        self.service
            .match_state(&state, scope.id(), &id)
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
        self.service
            .terminate_match(&state, &auth, scope.id(), &id)
            .await
            .into_response()
    }
}
