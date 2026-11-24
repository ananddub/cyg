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

type FeatureFlagRowList = Vec<crate::dto::flag::FeatureFlagRow>;
type VariantRowList = Vec<crate::dto::flag::VariantRow>;
type OverrideRowList = Vec<crate::dto::flag::OverrideRow>;
#[controller("/v1/console/flags")]
impl ConsoleFlagController {
    fn new(service: Arc<ConsoleService>) -> Self {
        Self { service }
    }

    #[get("", response_body = FeatureFlagRowList)]
    async fn list(&self, _auth: ConsoleAuthed) -> Response {
        let state = current_state();
        self.service.list_flags().await.into_response()
    }

    #[post("", response_body = crate::dto::flag::FeatureFlagRow)]
    async fn create(
        &self,
        auth: ConsoleAuthed,
        Json(body): JsonBody<FeatureFlagRequest>,
    ) -> Response {
        let state = current_state();
        self.service.create_flag(&auth, body).await.into_response()
    }

    #[put("/{key}", response_body = crate::dto::flag::FeatureFlagRow)]
    async fn update(
        &self,
        auth: ConsoleAuthed,
        Path(key): Path<String>,
        Json(body): JsonBody<FeatureFlagRequest>,
    ) -> Response {
        let state = current_state();
        self.service
            .update_flag(&auth, &key, body)
            .await
            .into_response()
    }

    #[delete("/{key}", response_body = crate::dto::Empty)]
    async fn delete(&self, auth: ConsoleAuthed, Path(key): Path<String>) -> Response {
        let state = current_state();
        self.service.delete_flag(&auth, &key).await.into_response()
    }

    #[post("/{key}/rollout", response_body = crate::dto::flag::RolloutRow)]
    async fn rollout(
        &self,
        auth: ConsoleAuthed,
        Path(key): Path<String>,
        Json(body): JsonBody<RolloutRequest>,
    ) -> Response {
        let state = current_state();
        self.service
            .set_rollout(&auth, &key, body)
            .await
            .into_response()
    }

    #[post("/{key}/stop", response_body = crate::dto::flag::RolloutRow)]
    async fn stop(&self, auth: ConsoleAuthed, Path(key): Path<String>) -> Response {
        let state = current_state();
        self.service.stop_flag(&auth, &key).await.into_response()
    }

    #[get("/{key}/variants", response_body = VariantRowList)]
    async fn list_variants(&self, _auth: ConsoleAuthed, Path(key): Path<String>) -> Response {
        let state = current_state();
        self.service.list_variants(&key).await.into_response()
    }

    #[post("/{key}/variants", response_body = crate::dto::flag::VariantRow)]
    async fn create_variant(
        &self,
        auth: ConsoleAuthed,
        Path(key): Path<String>,
        Json(body): JsonBody<VariantRequest>,
    ) -> Response {
        let state = current_state();
        self.service
            .create_variant(&auth, &key, body)
            .await
            .into_response()
    }

    #[put("/{key}/variants/{name}", response_body = crate::dto::flag::VariantRow)]
    async fn put_variant(
        &self,
        auth: ConsoleAuthed,
        Path((key, name)): Path<(String, String)>,
        Json(body): JsonBody<VariantRequest>,
    ) -> Response {
        let state = current_state();
        self.service
            .put_variant(&auth, &key, &name, body)
            .await
            .into_response()
    }

    #[delete("/{key}/variants/{name}", response_body = crate::dto::Empty)]
    async fn delete_variant(
        &self,
        auth: ConsoleAuthed,
        Path((key, name)): Path<(String, String)>,
    ) -> Response {
        let state = current_state();
        self.service
            .delete_variant(&auth, &key, &name)
            .await
            .into_response()
    }

    #[get("/{key}/overrides", response_body = OverrideRowList)]
    async fn list_overrides(&self, _auth: ConsoleAuthed, Path(key): Path<String>) -> Response {
        let state = current_state();
        self.service.list_overrides(&key).await.into_response()
    }

    #[put("/{key}/overrides", response_body = crate::dto::flag::OverrideRow)]
    async fn set_override(
        &self,
        auth: ConsoleAuthed,
        Path(key): Path<String>,
        Json(body): JsonBody<OverrideRequest>,
    ) -> Response {
        let state = current_state();
        self.service
            .set_override(&auth, &key, body)
            .await
            .into_response()
    }

    #[delete("/{key}/overrides/{user}", response_body = crate::dto::Empty)]
    async fn delete_override(
        &self,
        auth: ConsoleAuthed,
        Path((key, user)): Path<(String, String)>,
    ) -> Response {
        let state = current_state();
        self.service
            .delete_override(&auth, &key, &user)
            .await
            .into_response()
    }
}
