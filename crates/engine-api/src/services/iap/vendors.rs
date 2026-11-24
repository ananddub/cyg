use crate::services::plane::Plane;

use axum::Json;

use crate::app::middleware::Authed;
use crate::dto::iap::{PurchaseResponse, PurchaseTokenRequest};
use crate::error::V2Error;
use crate::utils::iap::store_of;

use super::IapService;
use super::purchases::record_purchase;
use super::verify::{self, GOOGLE};

impl IapService {
    pub async fn purchase_google(
        &self,

        claims: &Authed,
        body: PurchaseTokenRequest,
    ) -> Result<Json<PurchaseResponse>, V2Error> {
        record_token_purchase(&self.plane, claims, body, GOOGLE).await
    }

    pub async fn purchase_huawei(
        &self,

        claims: &Authed,
        body: PurchaseTokenRequest,
    ) -> Result<Json<PurchaseResponse>, V2Error> {
        record_token_purchase(&self.plane, claims, body, 2).await
    }

    pub async fn purchase_facebook_instant(
        &self,

        claims: &Authed,
        body: PurchaseTokenRequest,
    ) -> Result<Json<PurchaseResponse>, V2Error> {
        record_token_purchase(&self.plane, claims, body, 3).await
    }

    pub async fn purchase_samsung(
        &self,

        claims: &Authed,
        body: PurchaseTokenRequest,
    ) -> Result<Json<PurchaseResponse>, V2Error> {
        record_token_purchase(&self.plane, claims, body, 4).await
    }
}

pub(super) async fn record_token_purchase(
    plane: &Plane,
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
        &plane.control.iap,
        store,
        credential,
        body.product_id.as_deref(),
        None,
    )
    .await?;
    record_purchase(plane, claims.0.uuid()?, verified, store).await
}
