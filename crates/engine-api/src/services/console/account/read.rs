use axum::Json;

use crate::ApiState;
use crate::dto::console::Page;
use crate::dto::console::account::{AccountNoteRow, AccountRow, WalletLedgerRow};
use crate::error::V2Error;
use crate::repositories::console::ListWalletLedgerParams;

impl super::ConsoleService {
    pub async fn list_accounts(
        &self,
        state: &ApiState,
        page: Page,
    ) -> Result<Json<Vec<AccountRow>>, V2Error> {
        let rows = crate::services::games::tenant::list_accounts(
            state,
            page.game_id,
            page.limit(),
            page.offset(),
        )
        .await?;
        Ok(Json(rows.into_iter().map(AccountRow::from).collect()))
    }

    pub async fn get_account(
        &self,
        state: &ApiState,
        id: &str,
    ) -> Result<Json<AccountRow>, V2Error> {
        let user = super::queries(state).await?
            .get_user_by_id(super::account_id(id)?)
            .await?;
        Ok(Json(AccountRow::from(user)))
    }

    pub async fn list_account_notes(
        &self,
        state: &ApiState,
        id: &str,
    ) -> Result<Json<Vec<AccountNoteRow>>, V2Error> {
        let rows = super::queries(state).await?
            .list_user_notes(super::account_id(id)?)
            .await?;
        Ok(Json(rows.into_iter().map(AccountNoteRow::from).collect()))
    }

    pub async fn list_wallet_ledger(
        &self,
        state: &ApiState,
        id: &str,
        page: Page,
    ) -> Result<Json<Vec<WalletLedgerRow>>, V2Error> {
        let rows = super::queries(state).await?
            .list_wallet_ledger(&ListWalletLedgerParams {
                user_id: super::account_id(id)?,
                limit: page.limit(),
                offset: page.offset(),
            })
            .await?;
        Ok(Json(rows.into_iter().map(WalletLedgerRow::from).collect()))
    }
}
