//! The wallet change an operator grants one account, and the ledger entry that
//! records it.

use axum::Json;
use uuid::Uuid;

use crate::app::middleware::ConsoleAuthed;
use crate::dto::console::account::{AccountRow, UpdateWalletRequest};
use crate::error::V2Error;
use crate::repositories::iap::InsertWalletLedgerParams;
use crate::ApiState;

impl super::ConsoleService {
    /// Apply a wallet changeset. The document maps currency to *delta*, added to what
    /// the account holds, and the same map goes to the ledger — so the trail records
    /// what was granted, not merely what the total became.
    pub async fn update_account_wallet(
        &self,
        state: &ApiState,
        claims: &ConsoleAuthed,
        id: &str,
        body: UpdateWalletRequest,
    ) -> Result<Json<AccountRow>, V2Error> {
        claims.require_write()?;
        let id = super::account_id(id)?;
        let queries = super::queries(state)?;
        let user = queries.get_user_by_id(id).await?;

        let changes = body.changeset.as_object().ok_or_else(|| {
            V2Error::invalid_argument("The changeset must be an object of currency to amount.")
        })?;
        if changes.is_empty() {
            return Err(V2Error::invalid_argument("The changeset cannot be empty."));
        }
        // A read-modify-write: an operator grant is not the hot path a purchase is,
        // and the console is the only writer that arrives with a delta rather than a
        // whole document.
        let mut wallet = user.wallet.as_object().cloned().unwrap_or_default();
        for (currency, delta) in changes {
            let delta = delta.as_f64().ok_or_else(|| {
                V2Error::invalid_argument(format!("The {currency} amount must be a number."))
            })?;
            let held = wallet
                .get(currency)
                .and_then(serde_json::Value::as_f64)
                .unwrap_or(0.0);
            wallet.insert(currency.clone(), serde_json::json!(held + delta));
        }

        queries
            .update_user_wallet(serde_json::Value::Object(wallet), id)
            .await?;
        queries
            .insert_wallet_ledger(&InsertWalletLedgerParams {
                id: Uuid::new_v4(),
                user_id: id,
                changeset: body.changeset.clone(),
                metadata: if body.metadata.is_null() {
                    serde_json::json!({})
                } else {
                    body.metadata.clone()
                },
            })
            .await?;
        super::audit(
            state,
            claims,
            "account.wallet.update",
            "account",
            Some(id),
            &format!("Updated the wallet of {}.", user.username),
        )
        .await;

        // Read back rather than echo the computed document: a concurrent writer's
        // change would otherwise be reported as this handler's result.
        let updated = queries.get_user_by_id(id).await?;
        Ok(Json(AccountRow::from(updated)))
    }
}
