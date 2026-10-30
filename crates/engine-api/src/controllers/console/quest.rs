use auto_route::controller;
use axum::extract::{Json, Path};
use axum::response::{IntoResponse, Response};
use std::sync::Arc;

use crate::app::middleware::ConsoleAuthed;
use crate::current_state;
use crate::dto::console::quest::{QuestDefinitionRequest, SeasonRequest};
use crate::services::console::ConsoleService;

type JsonBody<T> = axum::extract::Json<T>;

struct ConsoleQuestController {
    service: Arc<ConsoleService>,
}

#[controller("/v2/console/quests")]
impl ConsoleQuestController {
    fn new(service: Arc<ConsoleService>) -> Self {
        Self { service }
    }

    #[get("")]
    async fn list(&self, _auth: ConsoleAuthed) -> Response {
        let state = current_state();
        self.service.list_quests(&state).await.into_response()
    }

    #[post("")]
    async fn create(
        &self,
        auth: ConsoleAuthed,
        Json(body): JsonBody<QuestDefinitionRequest>,
    ) -> Response {
        let state = current_state();
        self.service
            .create_quest(&state, &auth, body)
            .await
            .into_response()
    }

    #[put("/{key}")]
    async fn update(
        &self,
        auth: ConsoleAuthed,
        Path(key): Path<String>,
        Json(body): JsonBody<QuestDefinitionRequest>,
    ) -> Response {
        let state = current_state();
        self.service
            .update_quest(&state, &auth, &key, body)
            .await
            .into_response()
    }

    #[delete("/{key}")]
    async fn delete(&self, auth: ConsoleAuthed, Path(key): Path<String>) -> Response {
        let state = current_state();
        self.service
            .delete_quest(&state, &auth, &key)
            .await
            .into_response()
    }
}

struct ConsoleSeasonController {
    service: Arc<ConsoleService>,
}

#[controller("/v2/console/seasons")]
impl ConsoleSeasonController {
    fn new(service: Arc<ConsoleService>) -> Self {
        Self { service }
    }

    #[get("")]
    async fn list(&self, _auth: ConsoleAuthed) -> Response {
        let state = current_state();
        self.service.list_seasons(&state).await.into_response()
    }

    #[post("")]
    async fn create(&self, auth: ConsoleAuthed, Json(body): JsonBody<SeasonRequest>) -> Response {
        let state = current_state();
        self.service
            .create_season(&state, &auth, body)
            .await
            .into_response()
    }

    #[put("/{key}")]
    async fn update(
        &self,
        auth: ConsoleAuthed,
        Path(key): Path<String>,
        Json(body): JsonBody<SeasonRequest>,
    ) -> Response {
        let state = current_state();
        self.service
            .update_season(&state, &auth, &key, body)
            .await
            .into_response()
    }

    #[delete("/{key}")]
    async fn delete(&self, auth: ConsoleAuthed, Path(key): Path<String>) -> Response {
        let state = current_state();
        self.service
            .delete_season(&state, &auth, &key)
            .await
            .into_response()
    }
}
