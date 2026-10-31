use super::*;

#[controller("/v2/console/account")]
impl ConsoleAccountController {
    fn new(service: Arc<ConsoleService>) -> Self {
        Self { service }
    }

    #[get("")]
    async fn list(&self, _auth: ConsoleAuthed, Params(page): Params<Page>) -> Response {
        let state = current_state();
        self.service
            .list_accounts(&state, page)
            .await
            .into_response()
    }

    #[post("/import")]
    async fn import(
        &self,
        auth: ConsoleAuthed,
        Params(scope): Params<GameScope>,
        Body(body): Body<AccountExport>,
    ) -> Response {
        let state = current_state();
        self.service
            .import_account(&state, &auth, scope.id(), body)
            .await
            .into_response()
    }

    #[get("/{id}")]
    pub(super) async fn get(
        &self,
        _auth: ConsoleAuthed,
        Path(id): Path<String>,
        Params(scope): Params<GameScope>,
    ) -> Response {
        let state = current_state();
        let id = match crate::services::console::account_id(&id) {
            Ok(id) => id,
            Err(err) => return err.into_response(),
        };
        if let Err(err) = tenant::require_account(&state, scope.id(), id).await {
            return err.into_response();
        }
        self.service
            .get_account(&state, &id.to_string())
            .await
            .into_response()
    }

    #[delete("/{id}")]
    async fn delete(
        &self,
        auth: ConsoleAuthed,
        Path(id): Path<String>,
        Params(scope): Params<GameScope>,
    ) -> Response {
        let state = current_state();
        let id = match crate::services::console::account_id(&id) {
            Ok(id) => id,
            Err(err) => return err.into_response(),
        };
        if let Err(err) = tenant::require_account(&state, scope.id(), id).await {
            return err.into_response();
        }
        self.service
            .delete_account(&state, &auth, &id.to_string())
            .await
            .into_response()
    }

    #[get("/{id}/export")]
    async fn export(
        &self,
        auth: ConsoleAuthed,
        Path(id): Path<String>,
        Params(scope): Params<GameScope>,
    ) -> Response {
        let state = current_state();
        let id = match crate::services::console::account_id(&id) {
            Ok(id) => id,
            Err(err) => return err.into_response(),
        };
        if let Err(err) = tenant::require_account(&state, scope.id(), id).await {
            return err.into_response();
        }
        self.service
            .export_account(&state, &auth, &id.to_string())
            .await
            .into_response()
    }

    #[post("/{id}/import")]
    async fn import_one(
        &self,
        auth: ConsoleAuthed,
        Path(id): Path<String>,
        Params(scope): Params<GameScope>,
        Body(body): Body<AccountExport>,
    ) -> Response {
        let state = current_state();
        self.service
            .import_account_by_id(&state, &auth, &id, scope.id(), body)
            .await
            .into_response()
    }

    #[post("/{id}/ban")]
    async fn ban(
        &self,
        auth: ConsoleAuthed,
        Path(id): Path<String>,
        Params(scope): Params<GameScope>,
        Body(body): Body<BanAccountRequest>,
    ) -> Response {
        let state = current_state();
        let parsed = match crate::services::console::account_id(&id) {
            Ok(id) => id,
            Err(err) => return err.into_response(),
        };
        if let Err(err) = tenant::require_account(&state, scope.id(), parsed).await {
            return err.into_response();
        }
        self.service
            .ban_account(&state, &auth, &id, body)
            .await
            .into_response()
    }

    #[post("/{id}/unban")]
    async fn unban(
        &self,
        auth: ConsoleAuthed,
        Path(id): Path<String>,
        Params(scope): Params<GameScope>,
    ) -> Response {
        let state = current_state();
        let parsed = match crate::services::console::account_id(&id) {
            Ok(id) => id,
            Err(err) => return err.into_response(),
        };
        if let Err(err) = tenant::require_account(&state, scope.id(), parsed).await {
            return err.into_response();
        }
        self.service
            .unban_account(&state, &auth, &id)
            .await
            .into_response()
    }

    #[get("/{id}/note")]
    async fn notes(
        &self,
        _auth: ConsoleAuthed,
        Path(id): Path<String>,
        Params(scope): Params<GameScope>,
    ) -> Response {
        let state = current_state();
        let parsed = match crate::services::console::account_id(&id) {
            Ok(id) => id,
            Err(err) => return err.into_response(),
        };
        if let Err(err) = tenant::require_account(&state, scope.id(), parsed).await {
            return err.into_response();
        }
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
        Params(scope): Params<GameScope>,
        Body(body): Body<AddNoteRequest>,
    ) -> Response {
        let state = current_state();
        let parsed = match crate::services::console::account_id(&id) {
            Ok(id) => id,
            Err(err) => return err.into_response(),
        };
        if let Err(err) = tenant::require_account(&state, scope.id(), parsed).await {
            return err.into_response();
        }
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
        Params(scope): Params<GameScope>,
    ) -> Response {
        let state = current_state();
        let parsed = match crate::services::console::account_id(&id) {
            Ok(id) => id,
            Err(err) => return err.into_response(),
        };
        if let Err(err) = tenant::require_account(&state, scope.id(), parsed).await {
            return err.into_response();
        }
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
        let parsed = match crate::services::console::account_id(&id) {
            Ok(id) => id,
            Err(err) => return err.into_response(),
        };
        if let Err(err) = tenant::require_account(&state, page.game_id, parsed).await {
            return err.into_response();
        }
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
        Params(scope): Params<GameScope>,
        Body(body): Body<UpdateWalletRequest>,
    ) -> Response {
        let state = current_state();
        let parsed = match crate::services::console::account_id(&id) {
            Ok(id) => id,
            Err(err) => return err.into_response(),
        };
        if let Err(err) = tenant::require_account(&state, scope.id(), parsed).await {
            return err.into_response();
        }
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
        Params(scope): Params<GameScope>,
    ) -> Response {
        let state = current_state();
        let parsed = match crate::services::console::account_id(&id) {
            Ok(id) => id,
            Err(err) => return err.into_response(),
        };
        if let Err(err) = tenant::require_account(&state, scope.id(), parsed).await {
            return err.into_response();
        }
        self.service
            .delete_wallet_ledger(&state, &auth, &id, &wallet_id)
            .await
            .into_response()
    }
}
