use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::dto::webhook::EndpointRequest;
use crate::error::V2Error;
use crate::repositories::models::WebhookEndpoint;

use super::EVENTS;

/// Freezes what the operator typed into a row. The id is kept across an edit, so
/// deliveries already queued for this endpoint are never orphaned.
pub(super) fn draft(
    existing: Option<WebhookEndpoint>,
    body: EndpointRequest,
    now: DateTime<Utc>,
) -> Result<WebhookEndpoint, V2Error> {
    let label = body.label.trim().to_owned();
    if label.is_empty() || label.len() > 64 {
        return Err(V2Error::invalid_argument(
            "A webhook label is 1-64 characters.",
        ));
    }
    let url = valid_url(body.url.trim()).ok_or_else(|| {
        V2Error::invalid_argument("A webhook URL has to be an https address, or http on loopback.")
    })?;
    let secret = body.secret.unwrap_or_default().trim().to_owned();
    if secret.len() < 16 || secret.len() > 128 {
        return Err(V2Error::invalid_argument(
            "A webhook secret is 16-128 characters.",
        ));
    }
    let mut events: Vec<String> = Vec::with_capacity(body.events.len());
    for event in &body.events {
        let event = event.trim();
        if event != "*" && !EVENTS.contains(&event) {
            return Err(V2Error::invalid_argument(&format!(
                "A webhook event must be one of {}, or \"*\".",
                EVENTS.join(", ")
            )));
        }
        let name = event.to_owned();
        if !events.contains(&name) {
            events.push(name);
        }
    }
    if events.is_empty() {
        return Err(V2Error::invalid_argument(
            "A webhook endpoint subscribes to at least one event.",
        ));
    }
    Ok(WebhookEndpoint {
        id: existing.as_ref().map_or_else(Uuid::new_v4, |row| row.id),
        label,
        url,
        secret,
        events,
        is_active: body.is_active.unwrap_or(true),
        create_time: existing.map_or(now, |row| row.create_time),
        update_time: now,
    })
}

/// Loopback http is allowed because a receiver on this host is the only way to
/// watch a delivery land without publishing a public endpoint.
fn valid_url(raw: &str) -> Option<String> {
    let url = reqwest::Url::parse(raw).ok()?;
    if url.host_str().is_none() || !url.username().is_empty() {
        return None;
    }
    match url.scheme() {
        "https" => Some(raw.to_owned()),
        "http"
            if matches!(
                url.host_str(),
                Some("127.0.0.1") | Some("localhost") | Some("::1")
            ) =>
        {
            Some(raw.to_owned())
        }
        _ => None,
    }
}

/// The secret an operator did not bring: generated here, answered once, and kept
/// in the row only as a signing key.
pub(super) fn new_secret() -> String {
    let mut bytes = [0u8; 24];
    rand::RngCore::fill_bytes(&mut rand::thread_rng(), &mut bytes);
    format!("cyg_wh_{}", hex::encode(bytes))
}
