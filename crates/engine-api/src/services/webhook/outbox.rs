use axum::Json;
use chrono::Utc;
use serde_json::Value;
use uuid::Uuid;

use crate::ApiState;
use crate::app::middleware::ConsoleAuthed;
use crate::dto::webhook::{DeliveryListResponse, DeliveryRow};
use crate::error::V2Error;
use crate::repositories::models::{WebhookDelivery, WebhookEndpoint};
use crate::repositories::webhook::QUEUED;
use crate::services::console::{audit, id};
use crate::services::deps::q;

use super::WebhookService;

const PAGE: i64 = 100;

/// Queues `event` with every endpoint that asked for it. This writes rows and
/// nothing else: the network is left to the delivery loop, so a request never
/// waits on somebody else's receiver, and a receiver that is down cannot make
/// this one fail.
///
/// `key` is the fact's own identity — the account id, the transaction id, the
/// listing id — and it is what a replayed request cannot queue twice.
pub(crate) async fn emit(state: &ApiState, event: &str, key: &str, payload: Value) {
    let Ok(queries) = q(state).await else { return };
    let Ok(rows) = queries.active_endpoints().await else {
        tracing::warn!("[webhook] {event} could not be fanned out");
        return;
    };
    let endpoints: Vec<WebhookEndpoint> = rows
        .into_iter()
        .filter(|endpoint| asks_for(endpoint, event))
        .collect();
    if endpoints.is_empty() {
        return;
    }
    let event_id = format!("{event}:{key}");
    let now = Utc::now();
    for endpoint in endpoints {
        let row = WebhookDelivery {
            id: Uuid::new_v4(),
            endpoint_id: endpoint.id,
            event_id: event_id.clone(),
            event: event.to_owned(),
            payload: payload.clone(),
            attempt: 0,
            status: QUEUED.to_owned(),
            response_code: 0,
            last_error: String::new(),
            next_retry_at: now,
            create_time: now,
            update_time: now,
        };
        if let Err(err) = queries.queue_delivery(&row).await {
            tracing::warn!("[webhook] {event} could not be queued for {key}: {err}");
        }
    }
}

/// `*` is an endpoint asking for everything the control plane knows how to say.
pub(crate) fn asks_for(endpoint: &WebhookEndpoint, event: &str) -> bool {
    endpoint
        .events
        .iter()
        .any(|wanted| wanted == event || wanted == "*")
}

impl WebhookService {
    pub async fn list_deliveries(
        &self,
        state: &ApiState,
        endpoint_id: &str,
    ) -> Result<Json<DeliveryListResponse>, V2Error> {
        let id = id(endpoint_id, "webhook endpoint")?;
        let rows = q(state).await?.list_deliveries(id, PAGE).await?;
        Ok(Json(DeliveryListResponse {
            success: true,
            deliveries: rows.iter().map(DeliveryRow::from).collect(),
        }))
    }

    /// Hands a finished row back to the queue. The attempt count is not reset:
    /// an operator asking for the same fact a third time is not a new fact, and
    /// the ceiling is what stops a receiver that is down from being hammered.
    pub async fn retry_delivery(
        &self,
        state: &ApiState,
        claims: &ConsoleAuthed,
        endpoint_id: &str,
        delivery_id: &str,
    ) -> Result<Json<DeliveryRow>, V2Error> {
        claims.require_write()?;
        let endpoint_id = id(endpoint_id, "webhook endpoint")?;
        let delivery_id = id(delivery_id, "webhook delivery")?;
        let queries = q(state).await?;
        let Some(row) = queries.delivery(delivery_id).await? else {
            return Err(V2Error::not_found("Webhook delivery not found."));
        };
        if row.endpoint_id != endpoint_id {
            return Err(V2Error::not_found("Webhook delivery not found."));
        }
        if !queries.reopen_delivery(delivery_id).await? {
            return Err(V2Error::failed_precondition(
                "This delivery is being sent right now.",
            ));
        }
        audit(
            state,
            claims,
            "webhook.retry",
            "webhook",
            Some(endpoint_id),
            &format!("Requeued {} for delivery.", row.event),
        )
        .await;
        let row = queries.delivery(delivery_id).await?.unwrap_or(row);
        Ok(Json(DeliveryRow::from(&row)))
    }
}
