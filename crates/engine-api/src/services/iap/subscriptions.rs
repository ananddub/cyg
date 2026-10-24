//! Subscription recording and listing for `/v2/iap/subscription*`.

use axum::Json;
use base64::Engine;
use chrono::Utc;
use serde_json::Value;
use uuid::Uuid;

use crate::ApiState;
use crate::app::middleware::Authed;
use crate::dto::iap::{
    ListQuery, SubscriptionListResponse, SubscriptionRequest, SubscriptionResponse,
};
use crate::error::V2Error;
use crate::repositories::iap::{InsertSubscriptionParams, ListPurchasesParams};
use crate::repositories::models::Subscription;
use crate::services::auth::q;
use crate::utils::iap::{decode_offset, environment_of, epoch, hex_id, store_of};

use super::IapService;

impl IapService {
    pub async fn subscription_apple(
        &self,
        state: &ApiState,
        claims: &Authed,
        body: SubscriptionRequest,
    ) -> Result<Json<SubscriptionResponse>, V2Error> {
        record_subscription(state, claims.0.uuid()?, body, 0).await
    }

    pub async fn subscription_google(
        &self,
        state: &ApiState,
        claims: &Authed,
        body: SubscriptionRequest,
    ) -> Result<Json<SubscriptionResponse>, V2Error> {
        record_subscription(state, claims.0.uuid()?, body, 1).await
    }

    pub async fn subscription_huawei(
        &self,
        state: &ApiState,
        claims: &Authed,
        body: SubscriptionRequest,
    ) -> Result<Json<SubscriptionResponse>, V2Error> {
        record_subscription(state, claims.0.uuid()?, body, 2).await
    }

    pub async fn list_subscriptions(
        &self,
        state: &ApiState,
        claims: &Authed,
        req: ListQuery,
    ) -> Result<Json<SubscriptionListResponse>, V2Error> {
        let queries = q(state)?;
        let limit = i64::from(req.limit.unwrap_or(100).clamp(1, 1000));
        let offset = decode_offset(req.cursor.as_deref());
        let rows = queries
            .list_subscriptions(&ListPurchasesParams {
                user_id: claims.0.uuid()?,
                limit,
                offset,
            })
            .await?;
        let has_more = rows.len() as i64 == limit;
        let subscriptions: Vec<SubscriptionResponse> =
            rows.iter().map(SubscriptionResponse::from).collect();
        let next_cursor = if has_more {
            base64::engine::general_purpose::STANDARD.encode((offset + limit).to_string())
        } else {
            String::new()
        };
        Ok(Json(SubscriptionListResponse {
            subscriptions,
            next_cursor,
        }))
    }

    /// `GET /v2/iap/subscription/{product_id}` — the caller's most recent
    /// subscription for a product, the reference server's `GetSubscription`.
    /// The statement is hand-written (`db/manual/iap.sql`): FlashORM generates
    /// one lookup per key, and this is the second one.
    pub async fn subscription_by_product(
        &self,
        state: &ApiState,
        claims: &Authed,
        product_id: &str,
    ) -> Result<Json<SubscriptionResponse>, V2Error> {
        let uid = claims.0.uuid()?;
        let pool = state.db.as_ref().ok_or_else(V2Error::no_db)?;
        let row = sqlx::query_as::<_, Subscription>(
            "SELECT * FROM subscription WHERE user_id = $1 AND product_id = $2 \
             ORDER BY purchase_time DESC, original_transaction_id ASC LIMIT 1",
        )
        .bind(uid)
        .bind(product_id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| V2Error::not_found("Subscription not found."))?;
        Ok(Json(SubscriptionResponse::from(&row)))
    }
}

async fn record_subscription(
    state: &ApiState,
    uid: Uuid,
    body: SubscriptionRequest,
    default_store: i16,
) -> Result<Json<SubscriptionResponse>, V2Error> {
    let token = if !body.receipt.is_empty() {
        body.receipt.clone()
    } else if body
        .purchase_token
        .as_deref()
        .is_some_and(|t| !t.is_empty())
    {
        body.purchase_token.clone().unwrap_or_default()
    } else {
        return Err(V2Error::invalid_argument(
            "receipt or purchase token is required",
        ));
    };
    let queries = q(state)?;
    let row = queries
        .insert_subscription(&InsertSubscriptionParams {
            original_transaction_id: hex_id(&token),
            user_id: uid,
            store: store_of(body.store, default_store),
            environment: environment_of(&token, default_store),
            product_id: body.product_id.unwrap_or_default(),
            purchase_time: Utc::now(),
            expire_time: body.expire_time.unwrap_or_else(epoch),
            refund_time: epoch(),
            raw_response: Value::String(token.clone()),
        })
        .await?;
    Ok(Json(SubscriptionResponse::from(&row)))
}
