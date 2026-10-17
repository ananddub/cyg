//! Auto-registered bindings for `/v2/console/storage*`.

use std::sync::Arc;

use auto_route::controller;
use axum::extract::Path;
use axum::response::{IntoResponse, Response};

use crate::app::middleware::ConsoleAuthed;
use crate::app::params::Params;
use crate::current_state;
use crate::dto::console::Page;
use crate::services::console::ConsoleService;

struct ConsoleStorageController {
    service: Arc<ConsoleService>,
}

#[controller("/v2/console/storage")]
impl ConsoleStorageController {
    fn new(service: Arc<ConsoleService>) -> Self {
        Self { service }
    }

    #[get("")]
    async fn list(&self, _auth: ConsoleAuthed, Params(page): Params<Page>) -> Response {
        let state = current_state();
        self.service.list_storage(&state, page).await.into_response()
    }

    #[get("/collections")]
    async fn collections(&self, _auth: ConsoleAuthed) -> Response {
        let state = current_state();
        self.service
            .list_storage_collections(&state)
            .await
            .into_response()
    }

    #[get("/{collection}")]
    async fn collection(&self, _auth: ConsoleAuthed, Path(collection): Path<String>) -> Response {
        let state = current_state();
        self.service
            .list_storage_in_collection(&state, &collection)
            .await
            .into_response()
    }

    /// The owner is part of the key, so it is part of the path: two accounts can
    /// hold the same key in the same collection and only one of them is being
    /// deleted.
    #[delete("/object/{collection}/{key}/{user_id}")]
    async fn delete_object(
        &self,
        auth: ConsoleAuthed,
        Path((collection, key, user_id)): Path<(String, String, String)>,
    ) -> Response {
        let state = current_state();
        self.service
            .delete_storage_object(&state, &auth, &collection, &key, &user_id)
            .await
            .into_response()
    }

    #[delete("/collection/{collection}")]
    async fn delete_collection(
        &self,
        auth: ConsoleAuthed,
        Path(collection): Path<String>,
    ) -> Response {
        let state = current_state();
        self.service
            .delete_storage_collection(&state, &auth, &collection)
            .await
            .into_response()
    }
}
