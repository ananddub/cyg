//! `/v2/console/purchase` and `/v2/console/subscription` — the IAP ledger.

use axum::Json;

use crate::dto::console::Page;
use crate::dto::console::iap::{PurchaseRow, SubscriptionRow};
use crate::error::V2Error;
use crate::ApiState;

impl super::ConsoleService {
    pub async fn list_purchases(
        &self,
        state: &ApiState,
        page: Page,
    ) -> Result<Json<Vec<PurchaseRow>>, V2Error> {
        let rows = super::queries(state)?
            .list_all_purchases(page.limit(), page.offset())
            .await?;
        Ok(Json(rows.into_iter().map(PurchaseRow::from).collect()))
    }

    pub async fn list_subscriptions(
        &self,
        state: &ApiState,
        page: Page,
    ) -> Result<Json<Vec<SubscriptionRow>>, V2Error> {
        let rows = super::queries(state)?
            .list_all_subscriptions(page.limit(), page.offset())
            .await?;
        Ok(Json(rows.into_iter().map(SubscriptionRow::from).collect()))
    }
}
