use auto_route::controller;
use axum::extract::Json;
use axum::http::HeaderMap;
use axum::response::{IntoResponse, Response};
use std::sync::Arc;

use crate::app::middleware::{Authed, ConsoleAuthed, server_key, server_key_or_console};
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
        let actor = grant_actor(&headers, &state);
        let quantity = body.quantity.unwrap_or(1).to_string();
        let slot = body.slot_index.unwrap_or(-1).to_string();
        let metadata = body
            .metadata
            .as_ref()
            .map(|m| m.to_string())
            .unwrap_or_default();
        let mut claim = match crate::services::idempotency::begin(
            &state,
            &headers,
            "inventory.grant",
            &actor,
            &[
                &target.to_string(),
                &body.item_id,
                &quantity,
                &slot,
                &metadata,
            ],
        )
        .await
        {
            Ok(claim) => claim,
            Err(err) => return err.into_response(),
        };
        if let Some(saved) = claim.replayed() {
            return axum::Json(saved).into_response();
        }
        let instance_id = claim.operation_uuid().unwrap_or_else(uuid::Uuid::new_v4);
        let result = self
            .service
            .grant_item(&state, target, body, instance_id)
            .await;
        let stored = result
            .as_ref()
            .ok()
            .and_then(|json| serde_json::to_value(&json.0).ok());
        claim.settle(stored).await;
        result.into_response()
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

fn grant_actor(headers: &HeaderMap, state: &crate::ApiState) -> String {
    if server_key(headers, state).is_ok() {
        return "server".to_owned();
    }
    match crate::app::middleware::console_claims(headers) {
        Ok(claims) => format!("console:{}", claims.uuid().unwrap_or_else(uuid::Uuid::nil)),
        Err(_) => "server".to_owned(),
    }
}
