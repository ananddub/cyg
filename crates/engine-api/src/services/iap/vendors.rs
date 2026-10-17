//! Token-shaped vendor purchases for `/v2/iap/purchase/{google,huawei,
//! facebookinstant,samsung}` — every vendor whose credential is an opaque
//! string gets recorded identically, and only the default store code differs.
//!
//! The vendor set mirrors the reference server: all five validate purchases,
//! none of Facebook Instant or Samsung has a subscription route.

use axum::Json;
use serde_json::Value;

use crate::ApiState;
use crate::app::middleware::Authed;
use crate::dto::iap::{PurchaseResponse, PurchaseTokenRequest};
use crate::error::V2Error;
use crate::utils::iap::{hex_id, store_of};

use super::IapService;
use super::purchases::record_purchase;

impl IapService {
    pub async fn purchase_google(
        &self,
        state: &ApiState,
        claims: &Authed,
        body: PurchaseTokenRequest,
    ) -> Result<Json<PurchaseResponse>, V2Error> {
        record_token_purchase(state, claims, body, 1).await
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

/// The one write behind every token vendor: reject a missing credential, then
/// hand the shared row writer the credential's digest as the transaction id.
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
    let transaction_id = hex_id(credential);
    let raw = Value::String(credential.to_owned());
    record_purchase(
        state,
        claims.0.uuid()?,
        transaction_id,
        store_of(body.store, default_store),
        body.product_id.unwrap_or_default(),
        raw,
    )
    .await
}
