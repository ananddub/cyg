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

type BadgeDefinitionRowList = Vec<crate::dto::badge::BadgeDefinitionRow>;
#[controller("/v1/console/badges")]
impl ConsoleBadgeController {
    fn new(service: Arc<ConsoleService>) -> Self {
        Self { service }
    }

    #[get("", response_body = BadgeDefinitionRowList)]
    async fn list(&self, _auth: ConsoleAuthed) -> Response {
        let state = current_state();
        self.service.list_badges().await.into_response()
    }

    #[post("", response_body = crate::dto::badge::BadgeDefinitionRow)]
    async fn create(
        &self,
        auth: ConsoleAuthed,
        Json(body): JsonBody<BadgeDefinitionRequest>,
    ) -> Response {
        let state = current_state();
        self.service.create_badge(&auth, body).await.into_response()
    }

    #[put("/{key}", response_body = crate::dto::badge::BadgeDefinitionRow)]
    async fn update(
        &self,
        auth: ConsoleAuthed,
        Path(key): Path<String>,
        Json(body): JsonBody<BadgeDefinitionRequest>,
    ) -> Response {
        let state = current_state();
        self.service
            .update_badge(&auth, &key, body)
            .await
            .into_response()
    }

    #[delete("/{key}", response_body = crate::dto::Empty)]
    async fn delete(&self, auth: ConsoleAuthed, Path(key): Path<String>) -> Response {
        let state = current_state();
        self.service.delete_badge(&auth, &key).await.into_response()
    }
}
