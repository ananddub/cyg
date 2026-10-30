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
        if let Err(err) = guards::require_account(&state, scope.id(), id).await {
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
        if let Err(err) = guards::require_account(&state, scope.id(), id).await {
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
        if let Err(err) = guards::require_account(&state, scope.id(), id).await {
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
        if let Err(err) = guards::require_account(&state, scope.id(), parsed).await {
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
        if let Err(err) = guards::require_account(&state, scope.id(), parsed).await {
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
        if let Err(err) = guards::require_account(&state, scope.id(), parsed).await {
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
        if let Err(err) = guards::require_account(&state, scope.id(), parsed).await {
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
        if let Err(err) = guards::require_account(&state, scope.id(), parsed).await {
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
        if let Err(err) = guards::require_account(&state, page.game_id, parsed).await {
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
        headers: axum::http::HeaderMap,
        Path(id): Path<String>,
        Params(scope): Params<GameScope>,
        Body(body): Body<UpdateWalletRequest>,
    ) -> Response {
        let state = current_state();
        let parsed = match crate::services::console::account_id(&id) {
            Ok(id) => id,
            Err(err) => return err.into_response(),
        };
        if let Err(err) = guards::require_account(&state, scope.id(), parsed).await {
            return err.into_response();
        }
        let changeset = body.changeset.to_string();
        let metadata = body.metadata.to_string();
        let actor = format!("console:{}", auth.0.uuid().unwrap_or_else(uuid::Uuid::nil));
        let mut claim = match crate::services::idempotency::begin(
            &state,
            &headers,
            "console.wallet.update",
            &actor,
            &[&id, &changeset, &metadata],
        )
        .await
        {
            Ok(claim) => claim,
            Err(err) => return err.into_response(),
        };
        if let Some(saved) = claim.replayed() {
            return axum::Json(saved).into_response();
        }
        let ledger_id = claim.operation_uuid().unwrap_or_else(uuid::Uuid::new_v4);
        let result = self
            .service
            .update_account_wallet(&state, &auth, &id, body, ledger_id)
            .await;
        let stored = result
            .as_ref()
            .ok()
            .and_then(|json| serde_json::to_value(&json.0).ok());
        claim.settle(stored).await;
        result.into_response()
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
        if let Err(err) = guards::require_account(&state, scope.id(), parsed).await {
            return err.into_response();
        }
        self.service
            .delete_wallet_ledger(&state, &auth, &id, &wallet_id)
            .await
            .into_response()
    }
}
