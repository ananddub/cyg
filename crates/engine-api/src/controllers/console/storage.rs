use std::sync::Arc;

use auto_route::controller;
use axum::extract::Path;
use axum::response::{IntoResponse, Response};

use crate::app::middleware::ConsoleAuthed;
use crate::app::params::Params;
use crate::current_state;
use crate::dto::console::{GameScope, Page};
use crate::services::console::ConsoleService;
use crate::utils::guards;

struct ConsoleStorageController {
    service: Arc<ConsoleService>,
}

type StorageObjectRowList = Vec<crate::dto::console::storage::StorageObjectRow>;
type StringList = Vec<String>;
#[controller("/v1/console/storage")]
impl ConsoleStorageController {
    fn new(service: Arc<ConsoleService>) -> Self {
        Self { service }
    }

    #[get("", response_body = StorageObjectRowList)]
    async fn list(&self, _auth: ConsoleAuthed, Params(page): Params<Page>) -> Response {
        let state = current_state();
        self.service.list_storage(page).await.into_response()
    }

    #[get("/collections", response_body = StringList)]
    async fn collections(
        &self,
        _auth: ConsoleAuthed,
        Params(scope): Params<GameScope>,
    ) -> Response {
        let state = current_state();
        self.service
            .list_storage_collections(scope.id())
            .await
            .into_response()
    }

    #[get("/{collection}", response_body = StorageObjectRowList)]
    async fn collection(
        &self,
        _auth: ConsoleAuthed,
        Path(collection): Path<String>,
        Params(scope): Params<GameScope>,
    ) -> Response {
        let state = current_state();
        self.service
            .list_storage_in_collection(scope.id(), &collection)
            .await
            .into_response()
    }

    #[delete("/object/{collection}/{key}/{user_id}", response_body = crate::dto::Empty)]
    async fn delete_object(
        &self,
        auth: ConsoleAuthed,
        Path((collection, key, user_id)): Path<(String, String, String)>,
        Params(scope): Params<GameScope>,
    ) -> Response {
        let state = current_state();
        let owner = match crate::services::console::account_id(&user_id) {
            Ok(id) => id,
            Err(err) => return err.into_response(),
        };
        if let Err(err) = guards::require_account(&*state.plane(), scope.id(), owner).await {
            return err.into_response();
        }
        self.service
            .delete_storage_object(&auth, &collection, &key, &user_id)
            .await
            .into_response()
    }

    #[delete("/collection/{collection}", response_body = crate::dto::console::storage::DeletedCount)]
    async fn delete_collection(
        &self,
        auth: ConsoleAuthed,
        Path(collection): Path<String>,
        Params(scope): Params<GameScope>,
    ) -> Response {
        let state = current_state();
        self.service
            .delete_storage_collection(&auth, scope.id(), &collection)
            .await
            .into_response()
    }
}
