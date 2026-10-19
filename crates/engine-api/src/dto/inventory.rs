//! Data Transfer Objects for the Inventory API.

use serde::{Deserialize, Serialize};
use uuid::Uuid;
use crate::repositories::models::PlayerInventoryItem;

#[derive(Debug, Serialize, Deserialize)]
pub struct ListInventoryResponse {
    pub items: Vec<PlayerInventoryItem>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct EquipItemRequest {
    pub instance_id: Uuid,
    pub slot_index: i32,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ConsumeItemRequest {
    pub instance_id: Uuid,
    pub quantity: i32,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct GrantItemRequest {
    pub user_id: Option<Uuid>,
    pub item_id: String,
    pub quantity: Option<i32>,
    pub slot_index: Option<i32>,
    pub metadata: Option<serde_json::Value>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DefineItemRequest {
    pub item_id: String,
    pub name: String,
    pub category: String,
    pub max_stack: Option<i32>,
    pub rarity: Option<String>,
    pub properties: Option<serde_json::Value>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct InventoryOperationResponse {
    pub success: bool,
    pub message: String,
    pub item: Option<PlayerInventoryItem>,
}
