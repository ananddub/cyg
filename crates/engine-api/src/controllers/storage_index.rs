//! Auto-registered bindings for `/v2/storage/index*` — the storage-index plane.
//!
//! reference registers indexes from its runtime and queries them from the console;
//! this is the same surface over player sessions: define an index, list the
//! definitions, drop one, and read the objects an index matched.

use std::sync::Arc;

use auto_route::controller;
use axum::Json;
use axum::extract::Path;
use axum::response::{IntoResponse, Response};

use crate::current_state;
use crate::app::middleware::Authed;

use crate::dto::storage_index::{CreateIndexRequest, IndexQueryRequest};
use crate::services::storage::StorageService;

/// Aliases so `auto_route` does not require `poem_openapi::Type` from the plain
/// `serde` shapes; extraction and rejection are exactly `axum`'s.
type JsonBody<T> = axum::extract::Json<T>;

struct StorageIndexController {
    service: Arc<StorageService>,
}

#[controller("/v2/storage/index")]
impl StorageIndexController {
    fn new(service: Arc<StorageService>) -> Self {
        Self { service }
    }

    #[post("")]
    async fn create_index(&self, auth: Authed, Json(body): JsonBody<CreateIndexRequest>) -> Response {
        let state = current_state();
        self.service
            .create_index(&state, &auth, body)
            .await
            .into_response()
    }

    #[get("")]
    async fn list_indexes(&self, auth: Authed) -> Response {
        let state = current_state();
        self.service.list_indexes(&state, &auth).await.into_response()
    }

    #[delete("/{name}")]
    async fn delete_index(&self, auth: Authed, Path(name): Path<String>) -> Response {
        let state = current_state();
        self.service.delete_index(&state, &auth, &name).await.into_response()
    }

    #[post("/{name}/query")]
    async fn query_index(
        &self,
        auth: Authed,
        Path(name): Path<String>,
        Json(body): JsonBody<IndexQueryRequest>,
    ) -> Response {
        let state = current_state();
        self.service
            .query_index(&state, &auth, &name, body)
            .await
            .into_response()
    }
}
