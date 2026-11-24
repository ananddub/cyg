use auto_route::controller;
use axum::Json;
use axum::response::Response;
use std::sync::Arc;

use crate::current_state;
use crate::services::status::StatusService;

#[derive(poem_openapi::Object, serde::Serialize)]
struct HealthResponse {
    status: String,
    routing: String,
}

struct HealthController;

#[controller("/auto")]
impl HealthController {
    fn new() -> Self {
        Self
    }

    #[get("/health")]
    async fn health(&self) -> Json<HealthResponse> {
        Json(HealthResponse {
            status: "ok".to_owned(),
            routing: "auto".to_owned(),
        })
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
        self.service.readyz().await
    }
}
