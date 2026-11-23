use std::sync::Arc;

use auto_route::controller;
use axum::extract::Path;
use axum::response::{IntoResponse, Response};

use crate::app::middleware::Authed;
use crate::app::params::{Body, Params};
use crate::current_state;
use crate::dto::notification::{DeleteRequest, ListQuery};

use crate::services::notification::{NotificationService, SendRequest};

struct NotificationController {
    service: Arc<NotificationService>,
}

#[controller("/v1/notification")]
impl NotificationController {
    fn new(service: Arc<NotificationService>) -> Self {
        Self { service }
    }

    #[get("")]
    async fn list_notifications(&self, auth: Authed, Params(req): Params<ListQuery>) -> Response {
        let state = current_state();
        self.service.list(&auth, req).await.into_response()
    }

    #[delete("")]
    async fn delete_notifications(
        &self,
        auth: Authed,
        Params(pairs): Params<Vec<(String, String)>>,
        Body(mut req): Body<DeleteRequest>,
    ) -> Response {
        req.ids.extend(
            pairs
                .into_iter()
                .filter(|(key, _)| key == "ids")
                .map(|(_, value)| value),
        );
        let state = current_state();
        self.service
            .delete_many(&auth, req)
            .await
            .into_response()
    }

    /// Raise a notification from the game's own code.
    ///
    /// An empty `user_id` addresses the caller, which is the only target a player
    /// session may write to; naming another player is refused here and belongs on
    /// the console route.
    #[post("")]
    async fn send(&self, auth: Authed, Body(body): Body<SendRequest>) -> Response {
        let state = current_state();
        self.service.send(&auth, body).await.into_response()
    }

    #[delete("/{id}")]
    async fn delete_notification(&self, auth: Authed, Path(id): Path<String>) -> Response {
        let state = current_state();
        self.service
            .delete_one(&auth, &id)
            .await
            .into_response()
    }
}
