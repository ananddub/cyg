use auto_route::controller;
use axum::extract::Json;
use axum::http::HeaderMap;
use axum::response::{IntoResponse, Response};
use std::sync::Arc;

use crate::app::middleware::server_key;
use crate::current_state;
use crate::dto::quest::IngestEventsRequest;
use crate::services::quest::QuestService;

type JsonBody<T> = axum::extract::Json<T>;

/// Game-server events. A player session is never enough here: this is the door
/// through which the authoritative server credits progress.
struct EventsController {
    service: Arc<QuestService>,
}

#[controller("/v2/events")]
impl EventsController {
    fn new(service: Arc<QuestService>) -> Self {
        Self { service }
    }

    #[post("")]
    async fn ingest(
        &self,
        headers: HeaderMap,
        Json(body): JsonBody<IngestEventsRequest>,
    ) -> Response {
        let state = current_state();
        if let Err(err) = server_key(&headers, &state) {
            return err.into_response();
        }
        self.service
            .ingest_events(&state, body)
            .await
            .into_response()
    }
}
