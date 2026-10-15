//! Auto-registered binding for `GET /v2/console/setting`.

use std::sync::Arc;

use auto_route::controller;
use axum::response::{IntoResponse, Response};

use crate::app::middleware::ConsoleAuthed;
use crate::current_state;
use crate::services::console::ConsoleService;

struct ConsoleSettingController {
    service: Arc<ConsoleService>,
}

#[controller("/v2/console/setting")]
impl ConsoleSettingController {
    fn new(service: Arc<ConsoleService>) -> Self {
        Self { service }
    }

    #[get("")]
    async fn list(&self, _auth: ConsoleAuthed) -> Response {
        let state = current_state();
        self.service.list_settings(&state).await.into_response()
    }
}
