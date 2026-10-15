//! Auto-registered bindings for `/v2/iap*`.

use std::sync::Arc;

use auto_route::controller;
use axum::Json;
use axum::response::{IntoResponse, Response};

use crate::current_state;
use crate::app::middleware::Authed;

use crate::dto::iap::{ListQuery, PurchaseRequest, PurchaseTokenRequest, SubscriptionRequest};
use crate::services::iap::IapService;

/// Aliases so `auto_route` does not demand `poem_openapi::Type` from the plain
/// `serde` receipt/query types; extraction and rejection are exactly `axum`'s.
type JsonBody<T> = axum::extract::Json<T>;
type QueryParams<T> = axum::extract::Query<T>;

struct IapController {
    service: Arc<IapService>,
}

#[controller("/v2/iap")]
impl IapController {
    fn new(service: Arc<IapService>) -> Self {
        Self { service }
    }

    #[post("")]
    async fn validate_apple_legacy(
        &self,
        auth: Authed,
        Json(body): JsonBody<PurchaseRequest>,
    ) -> Response {
        let state = current_state();
        self.service
            .validate_apple_legacy(&state, &auth, body)
            .await
            .into_response()
    }

    #[post("/purchase/apple")]
    async fn purchase_apple(&self, auth: Authed, Json(body): JsonBody<PurchaseRequest>) -> Response {
        let state = current_state();
        self.service
            .purchase_apple(&state, &auth, body)
            .await
            .into_response()
    }

    #[post("/purchase/google")]
    async fn purchase_google(&self, auth: Authed, Json(body): JsonBody<PurchaseTokenRequest>) -> Response {
        let state = current_state();
        self.service
            .purchase_google(&state, &auth, body)
            .await
            .into_response()
    }

    #[post("/purchase/huawei")]
    async fn purchase_huawei(&self, auth: Authed, Json(body): JsonBody<PurchaseTokenRequest>) -> Response {
        let state = current_state();
        self.service
            .purchase_huawei(&state, &auth, body)
            .await
            .into_response()
    }

    #[get("/purchase")]
    async fn list_purchases(&self, auth: Authed, req: QueryParams<ListQuery>) -> Response {
        let state = current_state();
        self.service
            .list_purchases(&state, &auth, req.0)
            .await
            .into_response()
    }

    #[post("/subscription/apple")]
    async fn subscription_apple(
        &self,
        auth: Authed,
        Json(body): JsonBody<SubscriptionRequest>,
    ) -> Response {
        let state = current_state();
        self.service
            .subscription_apple(&state, &auth, body)
            .await
            .into_response()
    }

    #[post("/subscription/google")]
    async fn subscription_google(
        &self,
        auth: Authed,
        Json(body): JsonBody<SubscriptionRequest>,
    ) -> Response {
        let state = current_state();
        self.service
            .subscription_google(&state, &auth, body)
            .await
            .into_response()
    }

    #[post("/subscription/huawei")]
    async fn subscription_huawei(
        &self,
        auth: Authed,
        Json(body): JsonBody<SubscriptionRequest>,
    ) -> Response {
        let state = current_state();
        self.service
            .subscription_huawei(&state, &auth, body)
            .await
            .into_response()
    }

    #[get("/subscription")]
    async fn list_subscriptions(&self, auth: Authed, req: QueryParams<ListQuery>) -> Response {
        let state = current_state();
        self.service
            .list_subscriptions(&state, &auth, req.0)
            .await
            .into_response()
    }
}
