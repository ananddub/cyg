//! Auto-registered binding for `GET /v2/console/notification`.

use std::sync::Arc;

use auto_route::controller;
use axum::response::{IntoResponse, Response};

use crate::app::middleware::ConsoleAuthed;
use crate::app::params::Params;
use crate::current_state;
use crate::dto::console::notification::NotificationQuery;
use crate::services::console::ConsoleService;

struct ConsoleNotificationController {
    service: Arc<ConsoleService>,
}

#[controller("/v2/console/notification")]
impl ConsoleNotificationController {
    fn new(service: Arc<ConsoleService>) -> Self {
        Self { service }
    }

    #[get("")]
    async fn list(&self, _auth: ConsoleAuthed, Params(query): Params<NotificationQuery>) -> Response {
        let state = current_state();
        self.service
            .list_notifications(&state, query)
            .await
            .into_response()
    }
}
