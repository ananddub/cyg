use std::sync::Arc;

use auto_route::controller;
use axum::Json;
use axum::extract::Path;
use axum::response::{IntoResponse, Response};

use crate::app::middleware::Authed;
use crate::current_state;

use crate::dto::storage_index::{CreateIndexRequest, IndexQueryRequest};
use crate::services::storage::StorageService;

type JsonBody<T> = axum::extract::Json<T>;

struct StorageIndexController {
    service: Arc<StorageService>,
}

#[controller("/v1/storage/index")]
impl StorageIndexController {
    fn new(service: Arc<StorageService>) -> Self {
        Self { service }
    }

    #[post("", response_body = crate::dto::storage_index::StorageIndexDef)]
    async fn create_index(
        &self,
        auth: Authed,
        Json(body): JsonBody<CreateIndexRequest>,
    ) -> Response {
        let state = current_state();
        self.service.create_index(&auth, body).await.into_response()
    }

    #[get("", response_body = crate::dto::storage_index::StorageIndexDefinitions)]
    async fn list_indexes(&self, auth: Authed) -> Response {
        let state = current_state();
        self.service.list_indexes(&auth).await.into_response()
    }

    #[delete("/{name}", response_body = crate::dto::Empty)]
    async fn delete_index(&self, auth: Authed, Path(name): Path<String>) -> Response {
        let state = current_state();
        self.service
            .delete_index(&auth, &name)
            .await
            .into_response()
    }

    #[post("/{name}/query", response_body = crate::dto::storage_index::StorageIndexList)]
    async fn query_index(
        &self,
        auth: Authed,
        Path(name): Path<String>,
        Json(body): JsonBody<IndexQueryRequest>,
    ) -> Response {
        let state = current_state();
        self.service
            .query_index(&auth, &name, body)
            .await
            .into_response()
    }
}
