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

#[controller("/v1/marketplace/listings")]
impl ListingsController {
    fn new(service: Arc<MarketplaceService>) -> Self {
        Self { service }
    }

    #[post("", response_body = crate::dto::marketplace::ListingResponse)]
    async fn create(&self, auth: Authed, Json(body): JsonBody<ListItemsRequest>) -> Response {
        let state = current_state();
        self.service.list_item(&auth, body).await.into_response()
    }

    #[post("/{listing}/buy", response_body = crate::dto::marketplace::ListingResponse)]
    async fn buy(&self, auth: Authed, Path(listing): Path<String>) -> Response {
        let state = current_state();
        let listing = match record_id(&listing, "listing") {
            Ok(id) => id,
            Err(err) => return err.into_response(),
        };
        self.service.buy(&auth, listing).await.into_response()
    }

    #[post("/{listing}/cancel", response_body = crate::dto::marketplace::ListingResponse)]
    async fn cancel(&self, auth: Authed, Path(listing): Path<String>) -> Response {
        let state = current_state();
        let listing = match record_id(&listing, "listing") {
            Ok(id) => id,
            Err(err) => return err.into_response(),
        };
        self.service.cancel(&auth, listing).await.into_response()
    }
}

struct WagerController {
    service: Arc<MarketplaceService>,
}

/// Wagers move player currency without the players asking for it: this door is
/// for the authoritative server only.
#[controller("/v1/wager")]
impl WagerController {
    fn new(service: Arc<MarketplaceService>) -> Self {
        Self { service }
    }

    #[post("/create", response_body = crate::dto::marketplace::WagerResponse)]
    async fn create(
        &self,
        headers: HeaderMap,
        Json(body): JsonBody<WagerCreateRequest>,
    ) -> Response {
        let state = current_state();
        if let Err(err) = server_key(&headers, &*state.plane()) {
            return err.into_response();
        }
        self.service.create_wager(body).await.into_response()
    }

    #[post("/{wager}/settle", response_body = crate::dto::marketplace::WagerResponse)]
    async fn settle(
        &self,
        headers: HeaderMap,
        Path(wager): Path<String>,
        Json(body): JsonBody<WagerSettleRequest>,
    ) -> Response {
        let state = current_state();
        if let Err(err) = server_key(&headers, &*state.plane()) {
            return err.into_response();
        }
        let wager = match record_id(&wager, "wager") {
            Ok(id) => id,
            Err(err) => return err.into_response(),
        };
        self.service.settle_wager(wager, body).await.into_response()
    }

    #[post("/{wager}/refund", response_body = crate::dto::marketplace::WagerResponse)]
    async fn refund(&self, headers: HeaderMap, Path(wager): Path<String>) -> Response {
        let state = current_state();
        if let Err(err) = server_key(&headers, &*state.plane()) {
            return err.into_response();
        }
        let wager = match record_id(&wager, "wager") {
            Ok(id) => id,
            Err(err) => return err.into_response(),
        };
        self.service.refund_wager(wager).await.into_response()
    }
}
