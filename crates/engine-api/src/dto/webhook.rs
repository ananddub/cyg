use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::repositories::models::{WebhookDelivery, WebhookEndpoint};

/// What an operator writes. `secret` is only ever accepted, never returned: a
/// create answers it once (and generates one when the operator left it out) and
/// after that it lives only in the signing path.
#[derive(Debug, Deserialize)]
pub struct EndpointRequest {
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub secret: Option<String>,
    #[serde(default)]
    pub events: Vec<String>,
    #[serde(default)]
    pub is_active: Option<bool>,
}

#[derive(Debug, Serialize)]
pub struct EndpointRow {
    pub endpoint_id: Uuid,
    pub label: String,
    pub url: String,
    pub events: Vec<String>,
    pub is_active: bool,
    pub create_time: DateTime<Utc>,
    pub update_time: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
pub struct EndpointListResponse {
    pub success: bool,
    pub endpoints: Vec<EndpointRow>,
}

#[derive(Debug, Serialize)]
pub struct EndpointCreatedResponse {
    pub success: bool,
    pub endpoint: EndpointRow,
    /// Shown once, at creation: the only time anyone hears it.
    pub secret: String,
}

#[derive(Debug, Serialize)]
pub struct DeliveryRow {
    pub delivery_id: Uuid,
    pub endpoint_id: Uuid,
    pub event: String,
    pub event_id: String,
    pub attempt: i64,
    pub status: String,
    pub response_code: i64,
    pub last_error: String,
    pub next_retry_at: DateTime<Utc>,
    pub create_time: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
pub struct DeliveryListResponse {
    pub success: bool,
    pub deliveries: Vec<DeliveryRow>,
}

impl From<&WebhookEndpoint> for EndpointRow {
    fn from(row: &WebhookEndpoint) -> Self {
        Self {
            endpoint_id: row.id,
            label: row.label.clone(),
            url: row.url.clone(),
            events: row.events.clone(),
            is_active: row.is_active,
            create_time: row.create_time,
            update_time: row.update_time,
        }
    }
}

impl From<&WebhookDelivery> for DeliveryRow {
    fn from(row: &WebhookDelivery) -> Self {
        Self {
            delivery_id: row.id,
            endpoint_id: row.endpoint_id,
            event: row.event.clone(),
            event_id: row.event_id.clone(),
            attempt: row.attempt,
            status: row.status.clone(),
            response_code: row.response_code,
            last_error: row.last_error.clone(),
            next_retry_at: row.next_retry_at,
            create_time: row.create_time,
        }
    }
}
