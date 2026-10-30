use axum::Json;
use chrono::Utc;

use crate::ApiState;
use crate::app::middleware::ConsoleAuthed;
use crate::dto::Empty;
use crate::dto::webhook::{
    EndpointCreatedResponse, EndpointListResponse, EndpointRequest, EndpointRow,
};
use crate::error::V2Error;
use crate::infra::webhook;
use crate::services::console::{audit, id};
use crate::services::deps::q;

use super::EVENTS;
use super::WebhookService;
use super::drafting::{draft, new_secret};

impl WebhookService {
    pub async fn list_endpoints(
        &self,
        state: &ApiState,
    ) -> Result<Json<EndpointListResponse>, V2Error> {
        let rows = q(state).await?.list_webhook_endpoints().await?;
        Ok(Json(EndpointListResponse {
            success: true,
            endpoints: rows.iter().map(EndpointRow::from).collect(),
        }))
    }

    /// Says how a delivery is signed, so the receiver can be written from what
    /// the console shows rather than from a failure.
    pub async fn signing_spec(&self) -> Result<Json<serde_json::Value>, V2Error> {
        Ok(Json(serde_json::json!({
            "success": true,
            "events": EVENTS,
            "signing": webhook::spec(),
        })))
    }

    pub async fn create_endpoint(
        &self,
        state: &ApiState,
        claims: &ConsoleAuthed,
        mut body: EndpointRequest,
    ) -> Result<Json<EndpointCreatedResponse>, V2Error> {
        claims.require_write()?;
        if body.secret.as_deref().is_none_or(|s| s.trim().is_empty()) {
            body.secret = Some(new_secret());
        }
        let endpoint = draft(None, body, Utc::now())?;
        let queries = q(state).await?;
        if !queries.create_webhook_endpoint(&endpoint).await? {
            return Err(V2Error::already_exists(
                "A webhook endpoint with this id already exists.",
            ));
        }
        audit(
            state,
            claims,
            "webhook.create",
            "webhook",
            Some(endpoint.id),
            &format!("Registered webhook {} -> {}.", endpoint.label, endpoint.url),
        )
        .await;
        Ok(Json(EndpointCreatedResponse {
            success: true,
            endpoint: EndpointRow::from(&endpoint),
            secret: endpoint.secret,
        }))
    }

    /// Writes the endpoint back whole. Leaving `secret` out keeps the one on the
    /// row, because editing a URL must not silently change how deliveries sign.
    pub async fn update_endpoint(
        &self,
        state: &ApiState,
        claims: &ConsoleAuthed,
        endpoint_id: &str,
        mut body: EndpointRequest,
    ) -> Result<Json<EndpointRow>, V2Error> {
        claims.require_write()?;
        let id = id(endpoint_id, "webhook endpoint")?;
        let queries = q(state).await?;
        let Some(existing) = queries.webhook_endpoint(id).await? else {
            return Err(V2Error::not_found("Webhook endpoint not found."));
        };
        if body.secret.as_deref().is_none_or(|s| s.trim().is_empty()) {
            body.secret = Some(existing.secret.clone());
        }
        let row = draft(Some(existing), body, Utc::now())?;
        queries.update_webhook_endpoint(&row).await?;
        audit(
            state,
            claims,
            "webhook.update",
            "webhook",
            Some(row.id),
            &format!("Wrote webhook {}.", row.label),
        )
        .await;
        Ok(Json(EndpointRow::from(&row)))
    }

    pub async fn delete_endpoint(
        &self,
        state: &ApiState,
        claims: &ConsoleAuthed,
        endpoint_id: &str,
    ) -> Result<Json<Empty>, V2Error> {
        claims.require_write()?;
        let id = id(endpoint_id, "webhook endpoint")?;
        let queries = q(state).await?;
        let Some(existing) = queries.webhook_endpoint(id).await? else {
            return Err(V2Error::not_found("Webhook endpoint not found."));
        };
        queries.delete_webhook_endpoint(id).await?;
        audit(
            state,
            claims,
            "webhook.delete",
            "webhook",
            Some(id),
            &format!("Deleted webhook {}.", existing.label),
        )
        .await;
        Ok(Json(Empty {}))
    }
}
