//! Auto-registered bindings for `/v2/inventory`.

use std::sync::Arc;
use auto_route::controller;
use axum::extract::Json;
use axum::response::{IntoResponse, Response};

use crate::current_state;
use crate::app::middleware::Authed;
use crate::dto::inventory::{EquipItemRequest, ConsumeItemRequest, GrantItemRequest, DefineItemRequest};
use crate::services::inventory::InventoryService;

type JsonBody<T> = axum::extract::Json<T>;

struct InventoryController {
    service: Arc<InventoryService>,
}

#[controller("/v2/inventory")]
impl InventoryController {
    fn new(service: Arc<InventoryService>) -> Self {
        Self { service }
    }

    #[get("")]
    async fn list(&self, auth: Authed) -> Response {
        let state = current_state();
        self.service
            .get_inventory(&state, &auth)
            .await
            .into_response()
    }

    #[post("/equip")]
    async fn equip(&self, auth: Authed, Json(body): JsonBody<EquipItemRequest>) -> Response {
        let state = current_state();
        self.service
            .equip_item(&state, &auth, body)
            .await
            .into_response()
    }

    #[post("/consume")]
    async fn consume(&self, auth: Authed, Json(body): JsonBody<ConsumeItemRequest>) -> Response {
        let state = current_state();
        self.service
            .consume_item(&state, &auth, body)
            .await
            .into_response()
    }

    #[post("/grant")]
    async fn grant(&self, auth: Authed, Json(body): JsonBody<GrantItemRequest>) -> Response {
        let state = current_state();
        self.service
            .grant_item(&state, &auth, body)
            .await
            .into_response()
    }

    #[post("/define")]
    async fn define(&self, Json(body): JsonBody<DefineItemRequest>) -> Response {
        let state = current_state();
        self.service
            .define_item(&state, body)
            .await
            .into_response()
    }
}
