use super::*;

/// Where the control plane pushes its facts. `secret` is write-only: no endpoint
/// returns it after creation, because it is the only thing an attacker would
/// need to forge a delivery.
#[derive(Debug, Clone, Serialize, Deserialize, somnia::SurrealRecord)]
#[table("webhook_endpoint", schemaless)]
#[index(name = "webhook_endpoint_active", fields = "is_active")]
pub struct WebhookEndpoint {
    #[field(thing)]
    pub id: Uuid,
    pub label: String,
    pub url: String,
    pub secret: String,
    pub events: Vec<String>,
    pub is_active: bool,
    #[serde(with = "time::iso")]
    pub create_time: DateTime<Utc>,
    #[serde(with = "time::iso")]
    pub update_time: DateTime<Utc>,
}

/// One fact owed to one endpoint. Written in the request that made the fact
/// true and sent later by the outbox worker, never inline.
#[derive(Debug, Clone, Serialize, Deserialize, somnia::SurrealRecord)]
#[table("webhook_delivery", schemaless)]
#[index(name = "webhook_delivery_due", fields = "status, next_retry_at")]
#[index(
    name = "webhook_delivery_endpoint_event_uniq",
    fields = "endpoint_id, event_id",
    unique
)]
pub struct WebhookDelivery {
    #[field(thing)]
    pub id: Uuid,
    pub endpoint_id: Uuid,
    /// The fact's own identity, so a replayed fact cannot queue a second time.
    pub event_id: String,
    pub event: String,
    pub payload: serde_json::Value,
    pub attempt: i64,
    pub status: String,
    /// Zero until an endpoint actually answered.
    pub response_code: i64,
    pub last_error: String,
    #[serde(with = "time::iso")]
    pub next_retry_at: DateTime<Utc>,
    #[serde(with = "time::iso")]
    pub create_time: DateTime<Utc>,
    #[serde(with = "time::iso")]
    pub update_time: DateTime<Utc>,
}
