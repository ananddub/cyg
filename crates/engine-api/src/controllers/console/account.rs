//! Auto-registered bindings for `/v2/console/account*`.

use std::sync::Arc;

use auto_route::controller;
use axum::extract::Path;
use axum::response::{IntoResponse, Response};

use crate::app::middleware::ConsoleAuthed;
use crate::app::params::Params;
use crate::current_state;
use crate::dto::console::Page;
use crate::services::console::ConsoleService;

struct ConsoleAccountController {
    service: Arc<ConsoleService>,
}

#[controller("/v2/console/account")]
impl ConsoleAccountController {
    fn new(service: Arc<ConsoleService>) -> Self {
        Self { service }
    }

    #[get("")]
    async fn list(&self, _auth: ConsoleAuthed, Params(page): Params<Page>) -> Response {
        let state = current_state();
        self.service.list_accounts(&state, page).await.into_response()
    }

    #[get("/{id}")]
    async fn get(&self, _auth: ConsoleAuthed, Path(id): Path<String>) -> Response {
        let state = current_state();
        self.service.get_account(&state, &id).await.into_response()
    }

    #[get("/{id}/note")]
    async fn notes(&self, _auth: ConsoleAuthed, Path(id): Path<String>) -> Response {
        let state = current_state();
        self.service
            .list_account_notes(&state, &id)
            .await
            .into_response()
    }

    #[get("/{id}/wallet")]
    async fn wallet(
        &self,
        _auth: ConsoleAuthed,
        Path(id): Path<String>,
        Params(page): Params<Page>,
    ) -> Response {
        let state = current_state();
        self.service
            .list_wallet_ledger(&state, &id, page)
            .await
            .into_response()
    }
}
