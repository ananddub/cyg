use axum::Json;
use base64::Engine;
use chrono::Utc;
use uuid::Uuid;

use crate::ApiState;
use crate::app::middleware::Authed;
use crate::dto::iap::{ListQuery, PurchaseListResponse, PurchaseRequest, PurchaseResponse};
use crate::error::V2Error;
use crate::repositories::iap::{InsertPurchaseParams, ListPurchasesParams};
use crate::services::deps::q;
use crate::utils::iap::{decode_offset, epoch, store_of};

use super::IapService;
use super::verify;

impl IapService {
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
        let verified = verify::verify_purchase(
            &state.control.iap,
            verify::APPLE,
            &body.receipt,
            body.product_id.as_deref(),
            body.transaction_id.as_deref(),
        )
        .await?;
        record_purchase(
            state,
            claims.0.uuid()?,
            verified,
            store_of(body.store, verify::APPLE),
        )
        .await
    }

    pub async fn list_purchases(
        &self,
        state: &ApiState,
        claims: &Authed,
        req: ListQuery,
    ) -> Result<Json<PurchaseListResponse>, V2Error> {
        let queries = q(state).await?;
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
    verified: verify::VerifiedPurchase,
    store: i16,
) -> Result<Json<PurchaseResponse>, V2Error> {
    let queries = q(state).await?;
    let row = queries
        .insert_purchase(&InsertPurchaseParams {
            transaction_id: verified.transaction_id,
            user_id: uid,
            store,
            environment: verified.environment,
            product_id: verified.product_id,
            purchase_time: verified.purchase_time.unwrap_or_else(Utc::now),
            refund_time: epoch(),
            raw_response: verified.raw,
        })
        .await?;
    crate::services::webhook::emit(
        state,
        "iap.purchase.verified",
        &row.transaction_id,
        serde_json::json!({
            "user_id": row.user_id,
            "transaction_id": row.transaction_id,
            "product_id": row.product_id,
            "store": row.store,
            "environment": row.environment,
        }),
    )
    .await;
    Ok(Json(PurchaseResponse::from(&row)))
}
