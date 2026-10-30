use std::sync::Arc;

use auto_route::controller;
use axum::extract::{Json, Path};
use axum::http::HeaderMap;
use axum::response::{IntoResponse, Response};
use serde_json::Value;

use crate::app::middleware::{Authed, server_key};
use crate::current_state;
use crate::services::match_result::MatchResultService;

type JsonBody<T> = axum::extract::Json<T>;

/// The host's report of how a match ended. A player session is never enough:
/// only the authoritative server may say what a match scored.
struct MatchResultController {
    service: Arc<MatchResultService>,
}

#[controller("/v2/match")]
impl MatchResultController {
    fn new(service: Arc<MatchResultService>) -> Self {
        Self { service }
    }

    #[post("/{id}/result")]
    async fn result(
        &self,
        headers: HeaderMap,
        Path(id): Path<String>,
        Json(body): JsonBody<Value>,
    ) -> Response {
        let state = current_state();
        if let Err(err) = server_key(&headers, &state) {
            return err.into_response();
        }
        let mut claim = match crate::services::idempotency::begin(
            &state,
            &headers,
            "match.result",
            &id,
            &[&id, &body.to_string()],
        )
        .await
        {
            Ok(claim) => claim,
            Err(err) => return err.into_response(),
        };
        if let Some(saved) = claim.replayed() {
            return Json(saved).into_response();
        }
        let result = self.service.record_result(&state, &id, body).await;
        let stored = result
            .as_ref()
            .ok()
            .and_then(|json| serde_json::to_value(&json.0).ok());
        claim.settle(stored).await;
        result.into_response()
    }
}

/// Badges as the player holds them: derived from the counters a match result
/// raised, never from a list stored against the account.
struct BadgeController {
    service: Arc<MatchResultService>,
}

#[controller("/v2/badges")]
impl BadgeController {
    fn new(service: Arc<MatchResultService>) -> Self {
        Self { service }
    }

    #[get("")]
    async fn list(&self, auth: Authed) -> Response {
        let state = current_state();
        self.service
            .list_badges(&state, &auth)
            .await
            .into_response()
    }
}
