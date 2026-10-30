use auto_route::controller;
use axum::extract::{Json, Path};
use axum::response::{IntoResponse, Response};
use std::sync::Arc;

use crate::app::middleware::ConsoleAuthed;
use crate::current_state;
use crate::dto::badge::BadgeDefinitionRequest;
use crate::services::console::ConsoleService;

type JsonBody<T> = axum::extract::Json<T>;

struct ConsoleBadgeController {
    service: Arc<ConsoleService>,
}

#[controller("/v2/console/badges")]
impl ConsoleBadgeController {
    fn new(service: Arc<ConsoleService>) -> Self {
        Self { service }
    }

    #[get("")]
    async fn list(&self, _auth: ConsoleAuthed) -> Response {
        let state = current_state();
        self.service.list_badges(&state).await.into_response()
    }

    #[post("")]
    async fn create(
        &self,
        auth: ConsoleAuthed,
        Json(body): JsonBody<BadgeDefinitionRequest>,
    ) -> Response {
        let state = current_state();
        self.service
            .create_badge(&state, &auth, body)
            .await
            .into_response()
    }

    #[put("/{key}")]
    async fn update(
        &self,
        auth: ConsoleAuthed,
        Path(key): Path<String>,
        Json(body): JsonBody<BadgeDefinitionRequest>,
    ) -> Response {
        let state = current_state();
        self.service
            .update_badge(&state, &auth, &key, body)
            .await
            .into_response()
    }

    #[delete("/{key}")]
    async fn delete(&self, auth: ConsoleAuthed, Path(key): Path<String>) -> Response {
        let state = current_state();
        self.service
            .delete_badge(&state, &auth, &key)
            .await
            .into_response()
    }
}
