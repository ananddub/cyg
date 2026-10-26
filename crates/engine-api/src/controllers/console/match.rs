//! Auto-registered bindings for `/v2/console/match*`.
//!
//! Listing and inspecting a match are reads open to any console role; terminating
//! one is a write, gated in the service by `require_write`. The registry is in
//! memory, so none of these need a database.

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

    /// List running matches.
    #[get("")]
    async fn list(&self, _auth: ConsoleAuthed, Params(page): Params<Page>) -> Response {
        let state = current_state();
        self.service
            .list_matches(&state, page)
            .await
            .into_response()
    }

    /// Inspect one match's live state.
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

    /// Terminate one running match.
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
