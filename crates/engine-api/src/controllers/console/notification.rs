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
        headers: axum::http::HeaderMap,
        Params(scope): Params<GameScope>,
        Body(body): Body<SendNotificationRequest>,
    ) -> Response {
        let state = current_state();
        let actor = format!("console:{}", auth.0.uuid().unwrap_or_else(uuid::Uuid::nil));
        let content = body.content.to_string();
        let code = body.code.to_string();
        let mut claim = match crate::services::idempotency::begin(
            &state,
            &headers,
            "console.notification.send",
            &actor,
            &[&body.user_id, &body.subject, &code, &content],
        )
        .await
        {
            Ok(claim) => claim,
            Err(err) => return err.into_response(),
        };
        if let Some(saved) = claim.replayed() {
            return axum::Json(saved).into_response();
        }
        let id = claim.operation_uuid().unwrap_or_else(uuid::Uuid::new_v4);
        let result = self
            .service
            .send_notification(&state, &auth, scope.id(), body, id)
            .await;
        let stored = result
            .as_ref()
            .ok()
            .and_then(|json| serde_json::to_value(&json.0).ok());
        claim.settle(stored).await;
        result.into_response()
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
