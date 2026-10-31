use auto_route::controller;
use axum::Json;
use serde_json::json;

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
