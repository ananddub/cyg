use axum::Json;

use crate::ApiState;
use crate::app::middleware::Authed;
use crate::dto::iap::{PurchaseResponse, PurchaseTokenRequest};
use crate::error::V2Error;
use crate::utils::iap::store_of;

use super::purchases::record_purchase;
use super::verify::{self, GOOGLE};
use super::IapService;

impl IapService {
    pub async fn purchase_google(
        &self,
        state: &ApiState,
        claims: &Authed,
        body: PurchaseTokenRequest,
    ) -> Result<Json<PurchaseResponse>, V2Error> {
        record_token_purchase(state, claims, body, GOOGLE).await
    }

    pub async fn purchase_huawei(
        &self,
        state: &ApiState,
        claims: &Authed,
        body: PurchaseTokenRequest,
    ) -> Result<Json<PurchaseResponse>, V2Error> {
        record_token_purchase(state, claims, body, 2).await
    }

    pub async fn purchase_facebook_instant(
        &self,
        state: &ApiState,
        claims: &Authed,
        body: PurchaseTokenRequest,
    ) -> Result<Json<PurchaseResponse>, V2Error> {
        record_token_purchase(state, claims, body, 3).await
    }

    pub async fn purchase_samsung(
        &self,
        state: &ApiState,
        claims: &Authed,
        body: PurchaseTokenRequest,
    ) -> Result<Json<PurchaseResponse>, V2Error> {
        record_token_purchase(state, claims, body, 4).await
    }
}

pub(super) async fn record_token_purchase(
    state: &ApiState,
    claims: &Authed,
    body: PurchaseTokenRequest,
    default_store: i16,
) -> Result<Json<PurchaseResponse>, V2Error> {
    let credential = body.credential();
    if credential.is_empty() {
        return Err(V2Error::invalid_argument("purchase token cannot be empty"));
    }
    let store = store_of(body.store, default_store);
    let verified = verify::verify_purchase(
        &state.control.iap,
        store,
        credential,
        body.product_id.as_deref(),
        None,
    )
    .await?;
    record_purchase(state, claims.0.uuid()?, verified, store).await
}
