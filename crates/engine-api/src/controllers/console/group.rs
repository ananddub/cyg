//! Auto-registered bindings for `/v2/console/group*`.
//!
//! A group belongs to the game its creator plays in, so every route that names
//! one checks the workspace's tenant before reading or deleting it.

use std::sync::Arc;

use auto_route::controller;
use axum::extract::Path;
use axum::response::{IntoResponse, Response};

use crate::app::middleware::ConsoleAuthed;
use crate::app::params::Params;
use crate::current_state;
use crate::dto::console::{GameScope, Page};
use crate::services::console::ConsoleService;
use crate::services::games::tenant;

struct ConsoleGroupController {
    service: Arc<ConsoleService>,
}

#[controller("/v2/console/group")]
impl ConsoleGroupController {
    fn new(service: Arc<ConsoleService>) -> Self {
        Self { service }
    }

    #[get("")]
    async fn list(&self, _auth: ConsoleAuthed, Params(page): Params<Page>) -> Response {
        let state = current_state();
        self.service.list_groups(&state, page).await.into_response()
    }

    #[get("/{id}")]
    async fn get(
        &self,
        _auth: ConsoleAuthed,
        Path(id): Path<String>,
        Params(scope): Params<GameScope>,
    ) -> Response {
        let state = current_state();
        if let Err(err) = guard(&state, scope.id(), &id).await {
            return err.into_response();
        }
        self.service.get_group(&state, &id).await.into_response()
    }

    #[get("/{id}/member")]
    async fn members(
        &self,
        _auth: ConsoleAuthed,
        Path(id): Path<String>,
        Params(scope): Params<GameScope>,
    ) -> Response {
        let state = current_state();
        if let Err(err) = guard(&state, scope.id(), &id).await {
            return err.into_response();
        }
        self.service
            .list_group_members(&state, &id)
            .await
            .into_response()
    }

    #[get("/{id}/export")]
    async fn export(
        &self,
        auth: ConsoleAuthed,
        Path(id): Path<String>,
        Params(scope): Params<GameScope>,
    ) -> Response {
        let state = current_state();
        if let Err(err) = guard(&state, scope.id(), &id).await {
            return err.into_response();
        }
        self.service
            .export_group(&state, &auth, &id)
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
        if let Err(err) = guard(&state, scope.id(), &id).await {
            return err.into_response();
        }
        self.service
            .delete_group(&state, &auth, &id)
            .await
            .into_response()
    }
}

async fn guard(
    state: &crate::ApiState,
    game: Option<uuid::Uuid>,
    raw: &str,
) -> Result<(), crate::error::V2Error> {
    let id = crate::services::console::id(raw, "group")?;
    tenant::require_group(state, game, id).await
}
