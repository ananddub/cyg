use std::sync::Arc;

use auto_route::controller;
use axum::extract::Path;
use axum::response::{IntoResponse, Response};

use crate::app::middleware::ConsoleAuthed;
use crate::app::params::{Body, Params};
use crate::current_state;
use crate::dto::console::GameScope;
use crate::dto::console::notification::{NotificationQuery, SendNotificationRequest};
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
    async fn list(
        &self,
        _auth: ConsoleAuthed,
        Params(query): Params<NotificationQuery>,
    ) -> Response {
        let state = current_state();
        self.service
            .list_notifications(&state, query)
            .await
            .into_response()
    }

    #[post("")]
    async fn send(
        &self,
        auth: ConsoleAuthed,
        Params(scope): Params<GameScope>,
        Body(body): Body<SendNotificationRequest>,
    ) -> Response {
        let state = current_state();
        self.service
            .send_notification(&state, &auth, scope.id(), body)
            .await
            .into_response()
    }

    #[delete("/{id}")]
    async fn delete(
        &self,
        auth: ConsoleAuthed,
        Path(id): Path<String>,
        Params(scope): Params<GameScope>,
    ) -> Response {
        let state = current_state();
        self.service
            .delete_notification(&state, &auth, scope.id(), &id)
            .await
            .into_response()
    }
}
