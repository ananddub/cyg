//! Auto-registered bindings for `/v2/storage*`.
//!
//! Plane state comes from `current_state` because the auto-registered router is stateless.

use std::sync::Arc;

use auto_route::controller;
use axum::Json;
use axum::extract::{Path, Query};
use axum::response::{IntoResponse, Response};

use crate::current_state;
use crate::app::middleware::Authed;

use crate::dto::storage::{ListQuery, ObjectIdsRequest, ReadQuery, WriteRequest};
use crate::services::storage::StorageService;

/// Aliases so `auto_route` does not require `poem_openapi::Type` from the plain
/// `serde` storage shapes; extraction and rejection are exactly `axum`'s.
type JsonBody<T> = axum::extract::Json<T>;
type QueryParams<T> = axum::extract::Query<T>;

struct StorageController {
    service: Arc<StorageService>,
}

#[controller("/v2/storage")]
impl StorageController {
    fn new(service: Arc<StorageService>) -> Self {
        Self { service }
    }

    #[put("")]
    async fn write_objects(&self, auth: Authed, Json(body): JsonBody<WriteRequest>) -> Response {
        let state = current_state();
        self.service
            .write_objects(&state, &auth, body)
            .await
            .into_response()
    }

    #[post("")]
    async fn read_objects(&self, auth: Authed, Json(body): JsonBody<ObjectIdsRequest>) -> Response {
        let state = current_state();
        self.service
            .read_objects(&state, &auth, body)
            .await
            .into_response()
    }

    #[put("/delete")]
    async fn delete_objects(&self, auth: Authed, Json(body): JsonBody<ObjectIdsRequest>) -> Response {
        let state = current_state();
        self.service
            .delete_objects(&state, &auth, body)
            .await
            .into_response()
    }

    #[get("/{collection}")]
    async fn list_collection(
        &self,
        auth: Authed,
        Path(collection): Path<String>,
        Query(req): QueryParams<ListQuery>,
    ) -> Response {
        let state = current_state();
        self.service
            .list_collection(&state, &auth, &collection, req)
            .await
            .into_response()
    }

    #[get("/{collection}/{user_id}")]
    async fn list_user_objects(
        &self,
        auth: Authed,
        Path((collection, user_id)): Path<(String, String)>,
        Query(req): QueryParams<ListQuery>,
    ) -> Response {
        let state = current_state();
        self.service
            .list_user_objects(&state, &auth, &collection, &user_id, req)
            .await
            .into_response()
    }

    #[get("/{collection}/{key}/{user_id}")]
    async fn read_object(
        &self,
        auth: Authed,
        Path((collection, key, user_id)): Path<(String, String, String)>,
        Query(req): QueryParams<ReadQuery>,
    ) -> Response {
        let state = current_state();
        self.service
            .read_object(&state, &auth, &collection, &key, &user_id, req)
            .await
            .into_response()
    }
}
