use axum::Json;
use uuid::Uuid;

use crate::ApiState;
use crate::app::middleware::ConsoleAuthed;
use crate::dto::Empty;
use crate::dto::console::account::{AccountRow, UpdateWalletRequest};
use crate::error::V2Error;
use crate::services::deps::q;

impl super::ConsoleService {
    pub async fn update_account_wallet(
        &self,
        state: &ApiState,
        claims: &ConsoleAuthed,
        id: &str,
        body: UpdateWalletRequest,
    ) -> Result<Json<AccountRow>, V2Error> {
        claims.require_write()?;
        let id = super::account_id(id)?;
        let queries = q(state).await?;
        let user = queries.get_user_by_id(id).await?;

        let changes = body.changeset.as_object().ok_or_else(|| {
            V2Error::invalid_argument("The changeset must be an object of currency to amount.")
        })?;
        if changes.is_empty() {
            return Err(V2Error::invalid_argument("The changeset cannot be empty."));
        }
        let mut wallet = user.wallet.as_object().cloned().unwrap_or_default();
        for (currency, delta) in changes {
            let delta = delta.as_f64().ok_or_else(|| {
                V2Error::invalid_argument(format!("The {currency} amount must be a number."))
            })?;
            let held = wallet
                .get(currency)
                .and_then(serde_json::Value::as_f64)
                .unwrap_or(0.0);
            let value = held + delta;
            if value < 0.0 {
                return Err(V2Error::failed_precondition(format!(
                    "The {currency} balance cannot go negative."
                )));
            }
            wallet.insert(currency.clone(), serde_json::json!(value));
        }

        let metadata = if body.metadata.is_null() {
            serde_json::json!({})
        } else {
            body.metadata.clone()
        };
        let updated = queries
            .apply_wallet_change(id, changes, metadata, Uuid::new_v4())
            .await?
            .ok_or_else(|| {
                V2Error::failed_precondition("A currency balance cannot go negative.")
            })?;
        super::audit(
            state,
            claims,
            "account.wallet.update",
            "account",
            Some(id),
            &format!("Updated the wallet of {}.", user.username),
        )
        .await;

        Ok(Json(AccountRow::from(updated)))
    }

    pub async fn delete_wallet_ledger(
        &self,
        state: &ApiState,
        claims: &ConsoleAuthed,
        id: &str,
        wallet_id: &str,
    ) -> Result<Json<Empty>, V2Error> {
        claims.require_write()?;
        let id = super::account_id(id)?;
        let wallet_id = super::id(wallet_id, "wallet ledger")?;
        if wallet_id.is_nil() {
            return Err(V2Error::invalid_argument("Invalid wallet ledger id."));
        }
        let removed = q(state).await?
            .delete_wallet_ledger(wallet_id, id)
            .await
            .map_err(V2Error::from)?;
        if !removed {
            return Err(V2Error::not_found("Wallet ledger entry not found."));
        }
        super::audit(
            state,
            claims,
            "account.wallet.ledger.delete",
            "account",
            Some(id),
            "Deleted a wallet ledger entry.",
        )
        .await;
        Ok(Json(Empty {}))
    }
}
