//! `/v2/account/wallet` — the signed-in player reads their own currency document
//! and ledger, and merges metadata onto ledger entries they already own.
//!
//! BaaS parity: `server/core_wallet.go`. The currency keys of a wallet are reserved:
//! a player's write never moves a balance (that is what the console
//! `POST /v2/console/account/{id}/wallet` is for), so a self-service call cannot
//! mint currency. Mirroring reference, `updateWallets` is only ever reached by the
//! server and the console, and the negative-balance guard there is enforced here on
//! the console path too.

use axum::Json;
use uuid::Uuid;

use crate::ApiState;
use crate::app::middleware::Authed;
use crate::dto::account::{PlayerWalletResponse, UpdatePlayerWalletRequest};
use crate::dto::console::account::WalletLedgerRow;
use crate::error::V2Error;
use crate::repositories::console::ListWalletLedgerParams;

/// How much of a player's own ledger one read returns. A trail is browsed, not
/// paged, from the client.
const LEDGER_PAGE: i64 = 100;

impl super::AccountService {
    pub async fn get_wallet(
        &self,
        state: &ApiState,
        auth: &Authed,
    ) -> Result<Json<PlayerWalletResponse>, V2Error> {
        let queries = state.q.clone().ok_or_else(V2Error::no_db)?;
        let user = queries.get_user_by_id(auth.uid()).await?;
        let ledger = ledger_rows(&queries, auth.uid()).await?;
        Ok(Json(PlayerWalletResponse {
            wallet: user.wallet,
            ledger_items: ledger,
        }))
    }

    /// Merge metadata onto the caller's own ledger entries. A `wallet_items` map
    /// that would move a balance is refused: currency is the console's to grant.
    pub async fn update_wallet(
        &self,
        state: &ApiState,
        auth: &Authed,
        body: UpdatePlayerWalletRequest,
    ) -> Result<Json<PlayerWalletResponse>, V2Error> {
        let queries = state.q.clone().ok_or_else(V2Error::no_db)?;
        let pool = state.db.as_ref().ok_or_else(V2Error::no_db)?;

        if let Some(items) = body.wallet_items.as_object() {
            if !items.is_empty() {
                return Err(V2Error::permission_denied(
                    "Currency changes require a console write; a player may only merge ledger metadata.",
                ));
            }
        }
        if body.ledger_items.is_empty() {
            return Err(V2Error::invalid_argument(
                "There is nothing to write: send ledger metadata to merge.",
            ));
        }

        let uid = auth.uid();
        for item in &body.ledger_items {
            let ledger_id = Uuid::parse_str(&item.id)
                .map_err(|_| V2Error::invalid_argument("Invalid ledger item id."))?;
            let changed = sqlx::query(
                "UPDATE wallet_ledger SET metadata = $1, update_time = now() WHERE id = $2 AND user_id = $3;",
            )
            .bind(&item.metadata)
            .bind(ledger_id)
            .bind(uid)
            .execute(pool)
            .await
            .map_err(V2Error::from_sqlx)?;
            if changed.rows_affected() == 0 {
                return Err(V2Error::not_found(
                    "That ledger entry does not belong to this account.",
                ));
            }
        }

        let user = queries.get_user_by_id(uid).await?;
        let ledger = ledger_rows(&queries, uid).await?;
        Ok(Json(PlayerWalletResponse {
            wallet: user.wallet,
            ledger_items: ledger,
        }))
    }
}

async fn ledger_rows(
    queries: &crate::repositories::db::Queries,
    uid: Uuid,
) -> Result<Vec<WalletLedgerRow>, V2Error> {
    let rows = queries
        .list_wallet_ledger(&ListWalletLedgerParams {
            user_id: uid,
            limit: LEDGER_PAGE,
            offset: 0,
        })
        .await?;
    Ok(rows.into_iter().map(WalletLedgerRow::from).collect())
}
