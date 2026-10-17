//! Auto-registered bindings for `/v2/console/account*` — the list and detail, and
//! the writes an operator makes on one.
//!
//! Every write's authorization is decided by the service it calls, so a route cannot
//! reach a mutation without the role check that belongs to it.

use std::sync::Arc;

use auto_route::controller;
use axum::extract::Path;
use axum::response::{IntoResponse, Response};

use crate::app::middleware::ConsoleAuthed;
use crate::app::params::{Body, Params};
use crate::current_state;
use crate::dto::console::Page;
use crate::dto::console::account::{
    AccountExport, AddNoteRequest, BanAccountRequest, UpdateWalletRequest,
};
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

    #[post("/import")]
    async fn import(
        &self,
        auth: ConsoleAuthed,
        Body(body): Body<AccountExport>,
    ) -> Response {
        let state = current_state();
        self.service
            .import_account(&state, &auth, body)
            .await
            .into_response()
    }

    #[get("/{id}")]
    async fn get(&self, _auth: ConsoleAuthed, Path(id): Path<String>) -> Response {
        let state = current_state();
        self.service.get_account(&state, &id).await.into_response()
    }

    #[delete("/{id}")]
    async fn delete(&self, auth: ConsoleAuthed, Path(id): Path<String>) -> Response {
        let state = current_state();
        self.service
            .delete_account(&state, &auth, &id)
            .await
            .into_response()
    }

    #[get("/{id}/export")]
    async fn export(&self, auth: ConsoleAuthed, Path(id): Path<String>) -> Response {
        let state = current_state();
        self.service
            .export_account(&state, &auth, &id)
            .await
            .into_response()
    }

    #[post("/{id}/ban")]
    async fn ban(
        &self,
        auth: ConsoleAuthed,
        Path(id): Path<String>,
        Body(body): Body<BanAccountRequest>,
    ) -> Response {
        let state = current_state();
        self.service
            .ban_account(&state, &auth, &id, body)
            .await
            .into_response()
    }

    #[post("/{id}/unban")]
    async fn unban(&self, auth: ConsoleAuthed, Path(id): Path<String>) -> Response {
        let state = current_state();
        self.service
            .unban_account(&state, &auth, &id)
            .await
            .into_response()
    }

    #[get("/{id}/note")]
    async fn notes(&self, _auth: ConsoleAuthed, Path(id): Path<String>) -> Response {
        let state = current_state();
        self.service
            .list_account_notes(&state, &id)
            .await
            .into_response()
    }

    #[post("/{id}/note")]
    async fn add_note(
        &self,
        auth: ConsoleAuthed,
        Path(id): Path<String>,
        Body(body): Body<AddNoteRequest>,
    ) -> Response {
        let state = current_state();
        self.service
            .add_account_note(&state, &auth, &id, body)
            .await
            .into_response()
    }

    #[delete("/{id}/note/{note_id}")]
    async fn delete_note(
        &self,
        auth: ConsoleAuthed,
        Path((id, note_id)): Path<(String, String)>,
    ) -> Response {
        let state = current_state();
        self.service
            .delete_account_note(&state, &auth, &id, &note_id)
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

    #[post("/{id}/wallet")]
    async fn update_wallet(
        &self,
        auth: ConsoleAuthed,
        Path(id): Path<String>,
        Body(body): Body<UpdateWalletRequest>,
    ) -> Response {
        let state = current_state();
        self.service
            .update_account_wallet(&state, &auth, &id, body)
            .await
            .into_response()
    }

    #[delete("/{id}/wallet/{wallet_id}")]
    async fn delete_wallet(
        &self,
        auth: ConsoleAuthed,
        Path((id, wallet_id)): Path<(String, String)>,
    ) -> Response {
        let state = current_state();
        self.service
            .delete_wallet_ledger(&state, &auth, &id, &wallet_id)
            .await
            .into_response()
    }
}
