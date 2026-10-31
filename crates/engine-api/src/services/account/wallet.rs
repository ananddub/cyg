use axum::Json;
use uuid::Uuid;

use crate::ApiState;
use crate::app::middleware::Authed;
use crate::dto::account::{PlayerWalletResponse, UpdatePlayerWalletRequest};
use crate::dto::console::account::WalletLedgerRow;
use crate::error::V2Error;
use crate::repositories::console::ListWalletLedgerParams;

const LEDGER_PAGE: i64 = 100;

impl super::AccountService {
    pub async fn get_wallet(
        &self,
        state: &ApiState,
        auth: &Authed,
    ) -> Result<Json<PlayerWalletResponse>, V2Error> {
        let queries = crate::services::deps::q(state).await?;
        let user = queries.get_user_by_id(auth.uid()).await?;
        let ledger = ledger_rows(&queries, auth.uid()).await?;
        Ok(Json(PlayerWalletResponse {
            wallet: user.wallet,
            ledger_items: ledger,
        }))
    }

    pub async fn update_wallet(
        &self,
        state: &ApiState,
        auth: &Authed,
        body: UpdatePlayerWalletRequest,
    ) -> Result<Json<PlayerWalletResponse>, V2Error> {
        let queries = crate::services::deps::q(state).await?;

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
            let changed = queries
                .update_wallet_ledger_metadata(uid, ledger_id, item.metadata.clone())
                .await
                .map_err(V2Error::from)?;
            if changed.is_empty() {
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
