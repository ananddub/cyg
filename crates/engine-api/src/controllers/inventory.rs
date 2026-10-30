use auto_route::controller;
use axum::extract::Json;
use axum::http::HeaderMap;
use axum::response::{IntoResponse, Response};
use std::sync::Arc;

use crate::app::middleware::{Authed, ConsoleAuthed, server_key_or_console};
use crate::current_state;
use crate::dto::inventory::{
    ConsumeItemRequest, DefineItemRequest, EquipItemRequest, GrantItemRequest,
};
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
    async fn grant(&self, headers: HeaderMap, Json(body): JsonBody<GrantItemRequest>) -> Response {
        let state = current_state();
        if let Err(err) = server_key_or_console(&headers, &state) {
            return err.into_response();
        }
        let Some(target) = body.user_id else {
            return crate::error::V2Error::invalid_argument(
                "user_id is required for a server-side grant.",
            )
            .into_response();
        };
        self.service
            .grant_item(&state, target, body)
            .await
            .into_response()
    }

    #[post("/define")]
    async fn define(
        &self,
        auth: ConsoleAuthed,
        Json(body): JsonBody<DefineItemRequest>,
    ) -> Response {
        if let Err(err) = auth.require_write() {
            return err.into_response();
        }
        let state = current_state();
        self.service.define_item(&state, body).await.into_response()
    }
}
