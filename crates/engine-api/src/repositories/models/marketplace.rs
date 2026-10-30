use super::*;

#[derive(Debug, Clone, Serialize, Deserialize, somnia::SurrealRecord)]
#[table("market_listing", schemaless)]
#[index(name = "market_listing_seller", fields = "seller_id")]
pub struct MarketListing {
    #[field(thing)]
    pub id: Uuid,
    pub seller_id: Uuid,
    pub instance_id: Uuid,
    pub item_id: String,
    pub quantity: i32,
    pub price_currency: String,
    pub price: f64,
    pub state: String,
    #[serde(with = "time::iso")]
    pub create_time: DateTime<Utc>,
    #[serde(with = "time::iso")]
    pub update_time: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, somnia::SurrealRecord)]
#[table("wager", schemaless)]
#[index(name = "wager_room", fields = "room")]
pub struct Wager {
    #[field(thing)]
    pub id: Uuid,
    pub room: String,
    pub players: Vec<Uuid>,
    pub currency: String,
    pub amount_each: f64,
    pub state: String,
    pub outcome: serde_json::Value,
    #[serde(with = "time::iso")]
    pub create_time: DateTime<Utc>,
    #[serde(with = "time::iso")]
    pub update_time: DateTime<Utc>,
}
