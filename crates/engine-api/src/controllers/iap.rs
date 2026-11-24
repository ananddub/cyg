use std::sync::Arc;

use auto_route::controller;
use axum::Json;
use axum::extract::Path;
use axum::http::HeaderMap;
use axum::response::{IntoResponse, Response};

use crate::app::middleware::Authed;
use crate::current_state;

use crate::dto::iap::{ListQuery, PurchaseRequest, PurchaseTokenRequest, SubscriptionRequest};
use crate::services::iap::IapService;

type JsonBody<T> = axum::extract::Json<T>;
type QueryParams<T> = axum::extract::Query<T>;

macro_rules! idempotent_purchase {
    ($state:expr, $headers:expr, $auth:expr, $parts:expr, $call:expr) => {{
        let mut claim = match crate::services::idempotency::begin(
            &*$state.plane(),
            &$headers,
            "iap.purchase",
            &$auth.uid().to_string(),
            $parts,
        )
        .await
        {
            Ok(claim) => claim,
            Err(err) => return err.into_response(),
        };
        if let Some(saved) = claim.replayed() {
            return crate::app::middleware::replay_response(saved);
        }
        let result = $call.await;
        let stored = result.as_ref().ok().and_then(|json| {
            serde_json::to_string(&json.0)
                .ok()
                .map(|raw| crate::services::idempotency::Stored { status: 200, raw })
        });
        claim.settle(stored).await;
        return result.into_response();
    }};
}

fn receipt_parts(body: &PurchaseRequest) -> Vec<String> {
    vec![
        body.receipt.clone(),
        body.product_id.clone().unwrap_or_default(),
        body.transaction_id.clone().unwrap_or_default(),
        body.store.unwrap_or(-1).to_string(),
    ]
}

fn token_parts(body: &PurchaseTokenRequest) -> Vec<String> {
    vec![
        body.credential().to_owned(),
        body.product_id.clone().unwrap_or_default(),
        body.purchase_id.clone().unwrap_or_default(),
        body.store.unwrap_or(-1).to_string(),
    ]
}

struct IapController {
    service: Arc<IapService>,
}

#[controller("/v1/iap")]
impl IapController {
    fn new(service: Arc<IapService>) -> Self {
        Self { service }
    }

    #[post("", response_body = crate::dto::iap::PurchaseResponse)]
    async fn validate_apple_legacy(
        &self,
        auth: Authed,
        headers: HeaderMap,
        Json(body): JsonBody<PurchaseRequest>,
    ) -> Response {
        let state = current_state();
        let parts = receipt_parts(&body);
        idempotent_purchase!(
            state,
            headers,
            auth,
            &parts.iter().map(String::as_str).collect::<Vec<_>>(),
            self.service.validate_apple_legacy(&auth, body)
        )
    }

    #[post("/purchase/apple", response_body = crate::dto::iap::PurchaseResponse)]
    async fn purchase_apple(
        &self,
        auth: Authed,
        headers: HeaderMap,
        Json(body): JsonBody<PurchaseRequest>,
    ) -> Response {
        let state = current_state();
        let parts = receipt_parts(&body);
        idempotent_purchase!(
            state,
            headers,
            auth,
            &parts.iter().map(String::as_str).collect::<Vec<_>>(),
            self.service.purchase_apple(&auth, body)
        )
    }

    #[post("/purchase/google", response_body = crate::dto::iap::PurchaseResponse)]
    async fn purchase_google(
        &self,
        auth: Authed,
        headers: HeaderMap,
        Json(body): JsonBody<PurchaseTokenRequest>,
    ) -> Response {
        let state = current_state();
        let parts = token_parts(&body);
        idempotent_purchase!(
            state,
            headers,
            auth,
            &parts.iter().map(String::as_str).collect::<Vec<_>>(),
            self.service.purchase_google(&auth, body)
        )
    }

    #[post("/purchase/huawei", response_body = crate::dto::iap::PurchaseResponse)]
    async fn purchase_huawei(
        &self,
        auth: Authed,
        headers: HeaderMap,
        Json(body): JsonBody<PurchaseTokenRequest>,
    ) -> Response {
        let state = current_state();
        let parts = token_parts(&body);
        idempotent_purchase!(
            state,
            headers,
            auth,
            &parts.iter().map(String::as_str).collect::<Vec<_>>(),
            self.service.purchase_huawei(&auth, body)
        )
    }

    #[post("/purchase/facebookinstant", response_body = crate::dto::iap::PurchaseResponse)]
    async fn purchase_facebook_instant(
        &self,
        auth: Authed,
        headers: HeaderMap,
        Json(body): JsonBody<PurchaseTokenRequest>,
    ) -> Response {
        let state = current_state();
        let parts = token_parts(&body);
        idempotent_purchase!(
            state,
            headers,
            auth,
            &parts.iter().map(String::as_str).collect::<Vec<_>>(),
            self.service.purchase_facebook_instant(&auth, body)
        )
    }

    #[post("/purchase/samsung", response_body = crate::dto::iap::PurchaseResponse)]
    async fn purchase_samsung(
        &self,
        auth: Authed,
        headers: HeaderMap,
        Json(body): JsonBody<PurchaseTokenRequest>,
    ) -> Response {
        let state = current_state();
        let parts = token_parts(&body);
        idempotent_purchase!(
            state,
            headers,
            auth,
            &parts.iter().map(String::as_str).collect::<Vec<_>>(),
            self.service.purchase_samsung(&auth, body)
        )
    }

    #[get("/purchase", response_body = crate::dto::iap::PurchaseListResponse)]
    async fn list_purchases(&self, auth: Authed, req: QueryParams<ListQuery>) -> Response {
        let state = current_state();
        self.service
            .list_purchases(&auth, req.0)
            .await
            .into_response()
    }

    #[post("/subscription/apple", response_body = crate::dto::iap::SubscriptionResponse)]
    async fn subscription_apple(
        &self,
        auth: Authed,
        Json(body): JsonBody<SubscriptionRequest>,
    ) -> Response {
        let state = current_state();
        self.service
            .subscription_apple(&auth, body)
            .await
            .into_response()
    }

    #[post("/subscription/google", response_body = crate::dto::iap::SubscriptionResponse)]
    async fn subscription_google(
        &self,
        auth: Authed,
        Json(body): JsonBody<SubscriptionRequest>,
    ) -> Response {
        let state = current_state();
        self.service
            .subscription_google(&auth, body)
            .await
            .into_response()
    }

    #[post("/subscription/huawei", response_body = crate::dto::iap::SubscriptionResponse)]
    async fn subscription_huawei(
        &self,
        auth: Authed,
        Json(body): JsonBody<SubscriptionRequest>,
    ) -> Response {
        let state = current_state();
        self.service
            .subscription_huawei(&auth, body)
            .await
            .into_response()
    }

    #[get("/subscription", response_body = crate::dto::iap::SubscriptionListResponse)]
    async fn list_subscriptions(&self, auth: Authed, req: QueryParams<ListQuery>) -> Response {
        let state = current_state();
        self.service
            .list_subscriptions(&auth, req.0)
            .await
            .into_response()
    }

    #[get("/subscription/active", response_body = crate::dto::iap::ActiveSubscriptionResponse)]
    async fn active_subscriptions(&self, auth: Authed) -> Response {
        let state = current_state();
        self.service
            .active_entitlements(&auth)
            .await
            .into_response()
    }

    #[get("/subscription/{product_id}", response_body = crate::dto::iap::SubscriptionResponse)]
    async fn subscription_by_product(
        &self,
        auth: Authed,
        Path(product_id): Path<String>,
    ) -> Response {
        let state = current_state();
        self.service
            .subscription_by_product(&auth, &product_id)
            .await
            .into_response()
    }
}
