//! Inventory & Item Management Service (`/v2/inventory/*`).

use auto_di::singleton;
use axum::Json;
use uuid::Uuid;

use crate::ApiState;
use crate::app::middleware::Authed;
use crate::dto::inventory::*;
use crate::error::V2Error;

mod queries;

pub struct InventoryService;

#[singleton]
impl InventoryService {
    pub fn new() -> Self {
        Self
    }
}

impl InventoryService {
    pub async fn get_inventory(
        &self,
        state: &ApiState,
        auth: &Authed,
    ) -> Result<Json<ListInventoryResponse>, V2Error> {
        let queries = state.q.clone().ok_or_else(V2Error::no_db)?;
        let items = queries.list_inventory(auth.uid()).await?;
        Ok(Json(ListInventoryResponse { items }))
    }

    pub async fn equip_item(
        &self,
        state: &ApiState,
        auth: &Authed,
        req: EquipItemRequest,
    ) -> Result<Json<InventoryOperationResponse>, V2Error> {
        let queries = state.q.clone().ok_or_else(V2Error::no_db)?;
        let item = queries
            .get_inventory_item(req.instance_id)
            .await?
            .ok_or_else(|| V2Error::not_found("inventory item not found"))?;

        if item.user_id != auth.uid() {
            return Err(V2Error::permission_denied("you do not own this item"));
        }

        queries
            .set_inventory_slot(req.instance_id, req.slot_index)
            .await?;
        let updated = queries.get_inventory_item(req.instance_id).await?;

        Ok(Json(InventoryOperationResponse {
            success: true,
            message: format!("item equipped to slot {}", req.slot_index),
            item: updated,
        }))
    }

    pub async fn consume_item(
        &self,
        state: &ApiState,
        auth: &Authed,
        req: ConsumeItemRequest,
    ) -> Result<Json<InventoryOperationResponse>, V2Error> {
        if req.quantity <= 0 {
            return Err(V2Error::invalid_argument(
                "quantity to consume must be at least 1",
            ));
        }

        let queries = state.q.clone().ok_or_else(V2Error::no_db)?;
        let item = queries
            .get_inventory_item(req.instance_id)
            .await?
            .ok_or_else(|| V2Error::not_found("inventory item not found"))?;

        if item.user_id != auth.uid() {
            return Err(V2Error::permission_denied("you do not own this item"));
        }

        if item.quantity < req.quantity {
            return Err(V2Error::invalid_argument(format!(
                "insufficient quantity: requested {}, available {}",
                req.quantity, item.quantity
            )));
        }

        let new_qty = item.quantity - req.quantity;
        queries
            .update_inventory_quantity(req.instance_id, new_qty)
            .await?;
        let updated = queries.get_inventory_item(req.instance_id).await?;

        Ok(Json(InventoryOperationResponse {
            success: true,
            message: format!("consumed {} items", req.quantity),
            item: updated,
        }))
    }

    pub async fn grant_item(
        &self,
        state: &ApiState,
        auth: &Authed,
        req: GrantItemRequest,
    ) -> Result<Json<InventoryOperationResponse>, V2Error> {
        let target_user = req.user_id.unwrap_or_else(|| auth.uid());
        let queries = state.q.clone().ok_or_else(V2Error::no_db)?;

        // Ensure item definition exists
        let def = queries
            .get_item_definition(&req.item_id)
            .await?
            .ok_or_else(|| {
                V2Error::not_found(format!("item definition '{}' not found", req.item_id))
            })?;

        let qty = req.quantity.unwrap_or(1).clamp(1, def.max_stack);
        let slot = req.slot_index.unwrap_or(-1);
        let metadata = req.metadata.unwrap_or_else(|| serde_json::json!({}));

        let instance_id = Uuid::new_v4();
        let item = queries
            .grant_inventory_item(instance_id, target_user, &req.item_id, qty, slot, &metadata)
            .await?;

        Ok(Json(InventoryOperationResponse {
            success: true,
            message: format!("granted {}x {}", qty, def.name),
            item: Some(item),
        }))
    }

    pub async fn define_item(
        &self,
        state: &ApiState,
        req: DefineItemRequest,
    ) -> Result<Json<InventoryOperationResponse>, V2Error> {
        let queries = state.q.clone().ok_or_else(V2Error::no_db)?;
        let max_stack = req.max_stack.unwrap_or(1).max(1);
        let rarity = req.rarity.unwrap_or_else(|| "common".to_string());
        let properties = req.properties.unwrap_or_else(|| serde_json::json!({}));

        queries
            .upsert_item_definition(
                &req.item_id,
                &req.name,
                &req.category,
                max_stack,
                &rarity,
                &properties,
            )
            .await?;

        Ok(Json(InventoryOperationResponse {
            success: true,
            message: format!("item definition '{}' registered", req.item_id),
            item: None,
        }))
    }
}
