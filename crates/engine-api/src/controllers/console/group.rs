use std::sync::Arc;

use auto_route::controller;
use axum::extract::Path;
use axum::response::{IntoResponse, Response};

use crate::app::middleware::ConsoleAuthed;
use crate::app::params::Params;
use crate::current_state;
use crate::dto::console::{GameScope, Page};
use crate::services::console::ConsoleService;
use crate::utils::guards;

struct ConsoleGroupController {
    service: Arc<ConsoleService>,
}

type GroupRowList = Vec<crate::dto::console::group::GroupRow>;
type GroupMemberRowList = Vec<crate::dto::console::group::GroupMemberRow>;
#[controller("/v1/console/group")]
impl ConsoleGroupController {
    fn new(service: Arc<ConsoleService>) -> Self {
        Self { service }
    }

    #[get("", response_body = GroupRowList)]
    async fn list(&self, _auth: ConsoleAuthed, Params(page): Params<Page>) -> Response {
        let state = current_state();
        self.service.list_groups(page).await.into_response()
    }

    #[get("/{id}", response_body = crate::dto::console::group::GroupRow)]
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
        self.service.get_group(&id).await.into_response()
    }

    #[get("/{id}/member", response_body = GroupMemberRowList)]
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
        self.service.list_group_members(&id).await.into_response()
    }

    #[get("/{id}/export", response_body = crate::dto::console::group::GroupExport)]
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
        self.service.export_group(&auth, &id).await.into_response()
    }

    #[delete("/{id}", response_body = crate::dto::Empty)]
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
        self.service.delete_group(&auth, &id).await.into_response()
    }
}

async fn guard(
    state: &crate::ApiState,
    game: Option<uuid::Uuid>,
    raw: &str,
) -> Result<(), crate::error::V2Error> {
    let id = crate::services::console::account_id(raw)?;
    guards::require_group(&*state.plane(), game, id).await
}
