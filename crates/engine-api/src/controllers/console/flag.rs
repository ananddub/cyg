use auto_route::controller;
use axum::extract::{Json, Path};
use axum::response::{IntoResponse, Response};
use std::sync::Arc;

use crate::app::middleware::ConsoleAuthed;
use crate::current_state;
use crate::dto::flag::{FeatureFlagRequest, OverrideRequest, RolloutRequest, VariantRequest};
use crate::services::console::ConsoleService;

type JsonBody<T> = axum::extract::Json<T>;

/// The operator's side of an experiment: what is being tested, how far it has
/// been rolled out, which arms it splits into and which accounts are pinned.
struct ConsoleFlagController {
    service: Arc<ConsoleService>,
}

#[controller("/v2/console/flags")]
impl ConsoleFlagController {
    fn new(service: Arc<ConsoleService>) -> Self {
        Self { service }
    }

    #[get("")]
    async fn list(&self, _auth: ConsoleAuthed) -> Response {
        let state = current_state();
        self.service.list_flags(&state).await.into_response()
    }

    #[post("")]
    async fn create(
        &self,
        auth: ConsoleAuthed,
        Json(body): JsonBody<FeatureFlagRequest>,
    ) -> Response {
        let state = current_state();
        self.service
            .create_flag(&state, &auth, body)
            .await
            .into_response()
    }

    #[put("/{key}")]
    async fn update(
        &self,
        auth: ConsoleAuthed,
        Path(key): Path<String>,
        Json(body): JsonBody<FeatureFlagRequest>,
    ) -> Response {
        let state = current_state();
        self.service
            .update_flag(&state, &auth, &key, body)
            .await
            .into_response()
    }

    #[delete("/{key}")]
    async fn delete(&self, auth: ConsoleAuthed, Path(key): Path<String>) -> Response {
        let state = current_state();
        self.service
            .delete_flag(&state, &auth, &key)
            .await
            .into_response()
    }

    #[post("/{key}/rollout")]
    async fn rollout(
        &self,
        auth: ConsoleAuthed,
        Path(key): Path<String>,
        Json(body): JsonBody<RolloutRequest>,
    ) -> Response {
        let state = current_state();
        self.service
            .set_rollout(&state, &auth, &key, body)
            .await
            .into_response()
    }

    #[post("/{key}/stop")]
    async fn stop(&self, auth: ConsoleAuthed, Path(key): Path<String>) -> Response {
        let state = current_state();
        self.service
            .stop_flag(&state, &auth, &key)
            .await
            .into_response()
    }

    #[get("/{key}/variants")]
    async fn list_variants(&self, _auth: ConsoleAuthed, Path(key): Path<String>) -> Response {
        let state = current_state();
        self.service
            .list_variants(&state, &key)
            .await
            .into_response()
    }

    #[post("/{key}/variants")]
    async fn create_variant(
        &self,
        auth: ConsoleAuthed,
        Path(key): Path<String>,
        Json(body): JsonBody<VariantRequest>,
    ) -> Response {
        let state = current_state();
        self.service
            .create_variant(&state, &auth, &key, body)
            .await
            .into_response()
    }

    #[put("/{key}/variants/{name}")]
    async fn put_variant(
        &self,
        auth: ConsoleAuthed,
        Path((key, name)): Path<(String, String)>,
        Json(body): JsonBody<VariantRequest>,
    ) -> Response {
        let state = current_state();
        self.service
            .put_variant(&state, &auth, &key, &name, body)
            .await
            .into_response()
    }

    #[delete("/{key}/variants/{name}")]
    async fn delete_variant(
        &self,
        auth: ConsoleAuthed,
        Path((key, name)): Path<(String, String)>,
    ) -> Response {
        let state = current_state();
        self.service
            .delete_variant(&state, &auth, &key, &name)
            .await
            .into_response()
    }

    #[get("/{key}/overrides")]
    async fn list_overrides(&self, _auth: ConsoleAuthed, Path(key): Path<String>) -> Response {
        let state = current_state();
        self.service
            .list_overrides(&state, &key)
            .await
            .into_response()
    }

    #[put("/{key}/overrides")]
    async fn set_override(
        &self,
        auth: ConsoleAuthed,
        Path(key): Path<String>,
        Json(body): JsonBody<OverrideRequest>,
    ) -> Response {
        let state = current_state();
        self.service
            .set_override(&state, &auth, &key, body)
            .await
            .into_response()
    }

    #[delete("/{key}/overrides/{user}")]
    async fn delete_override(
        &self,
        auth: ConsoleAuthed,
        Path((key, user)): Path<(String, String)>,
    ) -> Response {
        let state = current_state();
        self.service
            .delete_override(&state, &auth, &key, &user)
            .await
            .into_response()
    }
}
