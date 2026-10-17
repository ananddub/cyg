//! Auto-registered bindings for `/v2/console/group*`.

use std::sync::Arc;

use auto_route::controller;
use axum::extract::Path;
use axum::response::{IntoResponse, Response};

use crate::app::middleware::ConsoleAuthed;
use crate::app::params::Params;
use crate::current_state;
use crate::dto::console::Page;
use crate::services::console::ConsoleService;

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
    async fn get(&self, _auth: ConsoleAuthed, Path(id): Path<String>) -> Response {
        let state = current_state();
        self.service.get_group(&state, &id).await.into_response()
    }

    #[get("/{id}/member")]
    async fn members(&self, _auth: ConsoleAuthed, Path(id): Path<String>) -> Response {
        let state = current_state();
        self.service
            .list_group_members(&state, &id)
            .await
            .into_response()
    }

    #[delete("/{id}")]
    async fn delete(&self, auth: ConsoleAuthed, Path(id): Path<String>) -> Response {
        let state = current_state();
        self.service
            .delete_group(&state, &auth, &id)
            .await
            .into_response()
    }
}
