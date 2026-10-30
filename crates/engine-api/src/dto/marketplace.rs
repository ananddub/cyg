use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::repositories::models::{MarketListing, Wager};

#[derive(Debug, Deserialize)]
pub struct ListItemsRequest {
    pub instance_id: Uuid,
    pub price_currency: String,
    pub price: f64,
}

#[derive(Debug, Deserialize)]
pub struct WagerCreateRequest {
    pub room: String,
    pub players: Vec<Uuid>,
    pub currency: String,
    pub amount_each: f64,
}

#[derive(Debug, Deserialize)]
pub struct WagerSettleRequest {
    pub winner: Uuid,
    #[serde(default)]
    pub data: serde_json::Value,
}

#[derive(Debug, Serialize)]
pub struct ListingRow {
    pub listing_id: Uuid,
    pub seller_id: Uuid,
    pub instance_id: Uuid,
    pub item_id: String,
    pub quantity: i32,
    pub price_currency: String,
    pub price: f64,
    pub state: String,
    pub create_time: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
pub struct WagerRow {
    pub wager_id: Uuid,
    pub room: String,
    pub players: Vec<Uuid>,
    pub currency: String,
    pub amount_each: f64,
    pub state: String,
    pub outcome: serde_json::Value,
    pub create_time: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
pub struct ListingResponse {
    pub success: bool,
    pub listing: ListingRow,
}

#[derive(Debug, Serialize)]
pub struct WagerResponse {
    pub success: bool,
    pub wager: WagerRow,
}

impl From<MarketListing> for ListingRow {
    fn from(row: MarketListing) -> Self {
        Self {
            listing_id: row.id,
            seller_id: row.seller_id,
            instance_id: row.instance_id,
            item_id: row.item_id,
            quantity: row.quantity,
            price_currency: row.price_currency,
            price: row.price,
            state: row.state,
            create_time: row.create_time,
        }
    }
}

impl From<Wager> for WagerRow {
    fn from(row: Wager) -> Self {
        Self {
            wager_id: row.id,
            room: row.room,
            players: row.players,
            currency: row.currency,
            amount_each: row.amount_each,
            state: row.state,
            outcome: row.outcome,
            create_time: row.create_time,
        }
    }
}
