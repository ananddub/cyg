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

type QuestDefinitionRowList = Vec<crate::dto::console::quest::QuestDefinitionRow>;
type SeasonRowList = Vec<crate::dto::console::quest::SeasonRow>;
#[controller("/v1/console/quests")]
impl ConsoleQuestController {
    fn new(service: Arc<ConsoleService>) -> Self {
        Self { service }
    }

    #[get("", response_body = QuestDefinitionRowList)]
    async fn list(&self, _auth: ConsoleAuthed) -> Response {
        let state = current_state();
        self.service.list_quests().await.into_response()
    }

    #[post("", response_body = crate::dto::console::quest::QuestDefinitionRow)]
    async fn create(
        &self,
        auth: ConsoleAuthed,
        Json(body): JsonBody<QuestDefinitionRequest>,
    ) -> Response {
        let state = current_state();
        self.service.create_quest(&auth, body).await.into_response()
    }

    #[put("/{key}", response_body = crate::dto::console::quest::QuestDefinitionRow)]
    async fn update(
        &self,
        auth: ConsoleAuthed,
        Path(key): Path<String>,
        Json(body): JsonBody<QuestDefinitionRequest>,
    ) -> Response {
        let state = current_state();
        self.service
            .update_quest(&auth, &key, body)
            .await
            .into_response()
    }

    #[delete("/{key}", response_body = crate::dto::Empty)]
    async fn delete(&self, auth: ConsoleAuthed, Path(key): Path<String>) -> Response {
        let state = current_state();
        self.service.delete_quest(&auth, &key).await.into_response()
    }
}

struct ConsoleSeasonController {
    service: Arc<ConsoleService>,
}

#[controller("/v1/console/seasons")]
impl ConsoleSeasonController {
    fn new(service: Arc<ConsoleService>) -> Self {
        Self { service }
    }

    #[get("", response_body = SeasonRowList)]
    async fn list(&self, _auth: ConsoleAuthed) -> Response {
        let state = current_state();
        self.service.list_seasons().await.into_response()
    }

    #[post("", response_body = crate::dto::console::quest::SeasonRow)]
    async fn create(&self, auth: ConsoleAuthed, Json(body): JsonBody<SeasonRequest>) -> Response {
        let state = current_state();
        self.service
            .create_season(&auth, body)
            .await
            .into_response()
    }

    #[put("/{key}", response_body = crate::dto::console::quest::SeasonRow)]
    async fn update(
        &self,
        auth: ConsoleAuthed,
        Path(key): Path<String>,
        Json(body): JsonBody<SeasonRequest>,
    ) -> Response {
        let state = current_state();
        self.service
            .update_season(&auth, &key, body)
            .await
            .into_response()
    }

    #[delete("/{key}", response_body = crate::dto::Empty)]
    async fn delete(&self, auth: ConsoleAuthed, Path(key): Path<String>) -> Response {
        let state = current_state();
        self.service
            .delete_season(&auth, &key)
            .await
            .into_response()
    }
}
