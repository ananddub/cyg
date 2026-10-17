//! Auto-registered bindings for `GET /v2/console/status` and
//! `GET /v2/console/route` — the console's own Status page and API explorer.

use std::sync::Arc;

use auto_route::controller;
use axum::response::{IntoResponse, Response};

use crate::app::middleware::ConsoleAuthed;
use crate::current_state;
use crate::services::console::ConsoleService;

struct ConsoleStatusController {
    service: Arc<ConsoleService>,
}

#[controller("/v2/console")]
impl ConsoleStatusController {
    fn new(service: Arc<ConsoleService>) -> Self {
        Self { service }
    }

    #[get("/status")]
    async fn status(&self, _auth: ConsoleAuthed) -> Response {
        let state = current_state();
        self.service.status(&state).await.into_response()
    }

    #[get("/route")]
    async fn routes(&self, _auth: ConsoleAuthed) -> Response {
        self.service.routes().await.into_response()
    }
}
