//! Auto-registered controllers for the telemetry surface: `/status` and `/api/rooms`.
//!
//! The routes live in [`crate::services::status`]; this file only binds them to
//! paths through `auto_route`.

use std::sync::Arc;

use auto_route::controller;
use axum::http::HeaderMap;
use axum::response::Response;

use crate::current_state;

use crate::services::status::StatusService;

struct StatusController {
    service: Arc<StatusService>,
}

#[controller("/status")]
impl StatusController {
    fn new(service: Arc<StatusService>) -> Self {
        Self { service }
    }

    #[get("")]
    async fn status(&self) -> Response {
        let state = current_state();
        self.service.status(&state).await
    }
}

struct RoomsController {
    service: Arc<StatusService>,
}

#[controller("/api")]
impl RoomsController {
    fn new(service: Arc<StatusService>) -> Self {
        Self { service }
    }

    #[get("/rooms")]
    async fn rooms(&self, headers: HeaderMap) -> Response {
        let state = current_state();
        self.service.rooms(&state, headers).await
    }
}
