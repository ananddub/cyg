//! Purchase recording and listing for the Apple-shaped `POST /v2/iap*purchase`
//! routes, the legacy `POST /v2/iap`, and the shared row write the token
//! vendors in [`super::vendors`] reuse.

use axum::Json;
use base64::Engine;
use chrono::Utc;
use serde_json::Value;
use uuid::Uuid;

use crate::ApiState;
use crate::app::middleware::Authed;
use crate::dto::iap::{ListQuery, PurchaseListResponse, PurchaseRequest, PurchaseResponse};
use crate::error::V2Error;
use crate::repositories::iap::{InsertPurchaseParams, ListPurchasesParams};
use crate::services::auth::q;
use crate::utils::iap::{decode_offset, environment_of, epoch, hex_id, store_of};

use super::IapService;

impl IapService {
    /// `POST /v2/iap` — the legacy Apple billing purchase call; same row, same
    /// rules as the explicit purchase route.
    pub async fn validate_apple_legacy(
        &self,
        state: &ApiState,
        claims: &Authed,
        body: PurchaseRequest,
    ) -> Result<Json<PurchaseResponse>, V2Error> {
        self.purchase_apple(state, claims, body).await
    }

    pub async fn purchase_apple(
        &self,
        state: &ApiState,
        claims: &Authed,
        body: PurchaseRequest,
    ) -> Result<Json<PurchaseResponse>, V2Error> {
        if body.receipt.is_empty() {
            return Err(V2Error::invalid_argument("receipt cannot be empty"));
        }
        let transaction_id = body
            .transaction_id
            .clone()
            .filter(|t| !t.is_empty())
            .unwrap_or_else(|| hex_id(&body.receipt));
        let raw = Value::String(body.receipt.clone());
        record_purchase(
            state,
            claims.0.uuid()?,
            transaction_id,
            store_of(body.store, 0),
            body.product_id.unwrap_or_default(),
            raw,
        )
        .await
    }

    pub async fn list_purchases(
        &self,
        state: &ApiState,
        claims: &Authed,
        req: ListQuery,
    ) -> Result<Json<PurchaseListResponse>, V2Error> {
        let queries = q(state)?;
        let limit = i64::from(req.limit.unwrap_or(100).clamp(1, 1000));
        let offset = decode_offset(req.cursor.as_deref());
        let rows = queries
            .list_purchases(&ListPurchasesParams {
                user_id: claims.0.uuid()?,
                limit,
                offset,
            })
            .await?;
        let has_more = rows.len() as i64 == limit;
        let purchases: Vec<PurchaseResponse> = rows.iter().map(PurchaseResponse::from).collect();
        let next_cursor = if has_more {
            base64::engine::general_purpose::STANDARD.encode((offset + limit).to_string())
        } else {
            String::new()
        };
        Ok(Json(PurchaseListResponse {
            purchases,
            next_cursor,
        }))
    }
}

pub(super) async fn record_purchase(
    state: &ApiState,
    uid: Uuid,
    transaction_id: String,
    store: i16,
    product_id: String,
    raw: Value,
) -> Result<Json<PurchaseResponse>, V2Error> {
    let queries = q(state)?;
    let row = queries
        .insert_purchase(&InsertPurchaseParams {
            transaction_id,
            user_id: uid,
            store,
            environment: environment_of(&raw.to_string(), store),
            product_id,
            purchase_time: Utc::now(),
            refund_time: epoch(),
            raw_response: raw,
        })
        .await?;
    Ok(Json(PurchaseResponse::from(&row)))
}
