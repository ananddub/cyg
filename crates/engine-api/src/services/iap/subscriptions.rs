use std::collections::BTreeSet;

use axum::Json;
use base64::Engine;
use chrono::{Duration, Utc};
use uuid::Uuid;

use crate::ApiState;
use crate::app::middleware::Authed;
use crate::dto::iap::{
    ActiveSubscriptionResponse, ActiveSubscriptionRow, ListQuery, SubscriptionListResponse,
    SubscriptionRequest, SubscriptionResponse,
};
use crate::error::V2Error;
use crate::infra::liveops;
use crate::repositories::iap::{InsertSubscriptionParams, ListPurchasesParams};
use crate::services::deps::q;
use crate::utils::iap::{decode_offset, epoch, store_of};

use super::IapService;
use super::entitlements::{is_active, state_of};
use super::verify::{self, APPLE, GOOGLE};

impl IapService {
    pub async fn subscription_apple(
        &self,
        state: &ApiState,
        claims: &Authed,
        body: SubscriptionRequest,
    ) -> Result<Json<SubscriptionResponse>, V2Error> {
        record_subscription(state, claims.0.uuid()?, body, APPLE).await
    }

    pub async fn subscription_google(
        &self,
        state: &ApiState,
        claims: &Authed,
        body: SubscriptionRequest,
    ) -> Result<Json<SubscriptionResponse>, V2Error> {
        record_subscription(state, claims.0.uuid()?, body, GOOGLE).await
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
        let queries = q(state).await?;
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

    pub async fn subscription_by_product(
        &self,
        state: &ApiState,
        claims: &Authed,
        product_id: &str,
    ) -> Result<Json<SubscriptionResponse>, V2Error> {
        let uid = claims.0.uuid()?;
        let queries = q(state).await?;
        let row = queries
            .subscription_by_product(uid, product_id)
            .await?
            .ok_or_else(|| V2Error::not_found("Subscription not found."))?;
        Ok(Json(SubscriptionResponse::from(&row)))
    }

    /// What the account is entitled to right now, and which rows the server has
    /// not heard from the store about in too long to trust on sight.
    pub async fn active_entitlements(
        &self,
        state: &ApiState,
        claims: &Authed,
    ) -> Result<Json<ActiveSubscriptionResponse>, V2Error> {
        let queries = q(state).await?;
        let now = Utc::now();
        let rows = queries.subscriptions_of(claims.0.uuid()?).await?;
        let stale_after = Duration::days(liveops::config().subscription_revalidation_days);
        let products: BTreeSet<String> = rows
            .iter()
            .filter(|row| is_active(row, now))
            .map(|row| row.product_id.clone())
            .collect();
        let subscriptions = rows
            .iter()
            .map(|row| ActiveSubscriptionRow::new(row, &now, stale_after))
            .collect();
        Ok(Json(ActiveSubscriptionResponse {
            success: true,
            active: !products.is_empty(),
            products: products.into_iter().collect(),
            subscriptions,
        }))
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
        .is_some_and(|token| !token.is_empty())
    {
        body.purchase_token.clone().unwrap_or_default()
    } else {
        return Err(V2Error::invalid_argument(
            "receipt or purchase token is required",
        ));
    };
    let store = store_of(body.store, default_store);
    let verified = verify::verify_purchase(
        &state.control.iap,
        store,
        &token,
        body.product_id.as_deref(),
        None,
    )
    .await?;
    let queries = q(state).await?;
    let row = queries
        .insert_subscription(&InsertSubscriptionParams {
            original_transaction_id: verified.transaction_id,
            user_id: uid,
            store,
            environment: verified.environment,
            product_id: verified.product_id,
            purchase_time: verified.purchase_time.unwrap_or_else(Utc::now),
            expire_time: body
                .expire_time
                .or(verified.expire_time)
                .unwrap_or_else(epoch),
            refund_time: epoch(),
            raw_response: verified.raw,
        })
        .await?;
    // A renewal rewrites the same row, so the expiry is what makes this fact a
    // different one from the record that landed last month.
    crate::services::webhook::emit(
        state,
        "subscription.state_changed",
        &format!(
            "{}:{}",
            row.original_transaction_id,
            row.expire_time.timestamp()
        ),
        serde_json::json!({
            "user_id": row.user_id,
            "original_transaction_id": row.original_transaction_id,
            "product_id": row.product_id,
            "store": row.store,
            "expire_time": row.expire_time,
            "state": state_of(&row, &Utc::now()),
            "active": is_active(&row, Utc::now()),
        }),
    )
    .await;
    Ok(Json(SubscriptionResponse::from(&row)))
}
