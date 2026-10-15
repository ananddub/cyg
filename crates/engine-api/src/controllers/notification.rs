//! Auto-registered bindings for `/v2/notification*`.
//!
//! Methods return `Response` (not `Json<T>`) so `auto_route` does not require
//! the payload type to implement `poem_openapi`'s `Type`.

use std::sync::Arc;

use auto_route::controller;
use axum::extract::Path;
use axum::response::{IntoResponse, Response};

use crate::current_state;
use crate::dto::notification::{DeleteRequest, ListQuery};
use crate::app::middleware::Authed;
use crate::app::params::Params;

use crate::services::notification::NotificationService;

struct NotificationController {
    service: Arc<NotificationService>,
}

#[controller("/v2/notification")]
impl NotificationController {
    fn new(service: Arc<NotificationService>) -> Self {
        Self { service }
    }

    #[get("")]
    async fn list_notifications(
        &self,
        auth: Authed,
        Params(req): Params<ListQuery>,
    ) -> Response {
        let state = current_state();
        self.service.list(&state, &auth, req).await.into_response()
    }

    #[delete("")]
    async fn delete_notifications(
        &self,
        auth: Authed,
        Params(req): Params<DeleteRequest>,
    ) -> Response {
        let state = current_state();
        self.service
            .delete_many(&state, &auth, req)
            .await
            .into_response()
    }

    #[delete("/{id}")]
    async fn delete_notification(
        &self,
        auth: Authed,
        Path(id): Path<String>,
    ) -> Response {
        let state = current_state();
        self.service.delete_one(&state, &auth, &id).await.into_response()
    }
}
