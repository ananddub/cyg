use auto_route::controller;
use axum::extract::{Json, Path};
use axum::http::HeaderMap;
use axum::response::{IntoResponse, Response};
use std::sync::Arc;
use uuid::Uuid;

use crate::app::middleware::{Authed, server_key};
use crate::current_state;
use crate::dto::marketplace::{ListItemsRequest, WagerCreateRequest, WagerSettleRequest};
use crate::error::V2Error;
use crate::services::marketplace::MarketplaceService;

type JsonBody<T> = axum::extract::Json<T>;

fn record_id(raw: &str, what: &str) -> Result<Uuid, V2Error> {
    raw.parse::<Uuid>()
        .map_err(|_| V2Error::invalid_argument(format!("A {what} id must be a record id.")))
}

struct ListingsController {
    service: Arc<MarketplaceService>,
}

#[controller("/v2/marketplace/listings")]
impl ListingsController {
    fn new(service: Arc<MarketplaceService>) -> Self {
        Self { service }
    }

    #[post("")]
    async fn create(&self, auth: Authed, Json(body): JsonBody<ListItemsRequest>) -> Response {
        let state = current_state();
        self.service
            .list_item(&state, &auth, body)
            .await
            .into_response()
    }

    #[post("/{listing}/buy")]
    async fn buy(&self, auth: Authed, Path(listing): Path<String>) -> Response {
        let state = current_state();
        let listing = match record_id(&listing, "listing") {
            Ok(id) => id,
            Err(err) => return err.into_response(),
        };
        self.service
            .buy(&state, &auth, listing)
            .await
            .into_response()
    }

    #[post("/{listing}/cancel")]
    async fn cancel(&self, auth: Authed, Path(listing): Path<String>) -> Response {
        let state = current_state();
        let listing = match record_id(&listing, "listing") {
            Ok(id) => id,
            Err(err) => return err.into_response(),
        };
        self.service
            .cancel(&state, &auth, listing)
            .await
            .into_response()
    }
}

struct WagerController {
    service: Arc<MarketplaceService>,
}

/// Wagers move player currency without the players asking for it: this door is
/// for the authoritative server only.
#[controller("/v2/wager")]
impl WagerController {
    fn new(service: Arc<MarketplaceService>) -> Self {
        Self { service }
    }

    #[post("/create")]
    async fn create(
        &self,
        headers: HeaderMap,
        Json(body): JsonBody<WagerCreateRequest>,
    ) -> Response {
        let state = current_state();
        if let Err(err) = server_key(&headers, &state) {
            return err.into_response();
        }
        self.service
            .create_wager(&state, body)
            .await
            .into_response()
    }

    #[post("/{wager}/settle")]
    async fn settle(
        &self,
        headers: HeaderMap,
        Path(wager): Path<String>,
        Json(body): JsonBody<WagerSettleRequest>,
    ) -> Response {
        let state = current_state();
        if let Err(err) = server_key(&headers, &state) {
            return err.into_response();
        }
        let wager = match record_id(&wager, "wager") {
            Ok(id) => id,
            Err(err) => return err.into_response(),
        };
        self.service
            .settle_wager(&state, wager, body)
            .await
            .into_response()
    }

    #[post("/{wager}/refund")]
    async fn refund(&self, headers: HeaderMap, Path(wager): Path<String>) -> Response {
        let state = current_state();
        if let Err(err) = server_key(&headers, &state) {
            return err.into_response();
        }
        let wager = match record_id(&wager, "wager") {
            Ok(id) => id,
            Err(err) => return err.into_response(),
        };
        self.service
            .refund_wager(&state, wager)
            .await
            .into_response()
    }
}
