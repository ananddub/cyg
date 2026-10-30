use auto_route::controller;
use axum::Json;
use axum::response::Response;
use serde_json::json;
use std::sync::Arc;

use crate::current_state;
use crate::services::status::StatusService;

struct HealthController;

#[controller("/auto")]
impl HealthController {
    fn new() -> Self {
        Self
    }

    #[get("/health")]
    async fn health(&self) -> Json<serde_json::Value> {
        Json(json!({
            "status": "ok",
            "routing": "auto",
        }))
    }
}

struct ReadyzController {
    service: Arc<StatusService>,
}

#[controller("/readyz")]
impl ReadyzController {
    fn new(service: Arc<StatusService>) -> Self {
        Self { service }
    }

    #[get("")]
    async fn readyz(&self) -> Response {
        let state = current_state();
        self.service.readyz(&state).await
    }
}
