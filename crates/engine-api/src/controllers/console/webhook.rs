use auto_route::controller;
use axum::extract::{Json, Path};
use axum::response::{IntoResponse, Response};
use std::sync::Arc;

use crate::app::middleware::ConsoleAuthed;
use crate::current_state;
use crate::dto::webhook::EndpointRequest;
use crate::services::webhook::WebhookService;

type JsonBody<T> = axum::extract::Json<T>;

struct ConsoleWebhookController {
    service: Arc<WebhookService>,
}

#[controller("/v2/console/webhooks")]
impl ConsoleWebhookController {
    fn new(service: Arc<WebhookService>) -> Self {
        Self { service }
    }

    #[get("")]
    async fn list(&self, _auth: ConsoleAuthed) -> Response {
        let state = current_state();
        self.service.list_endpoints(&state).await.into_response()
    }

    /// The secret is answered here and never again, so this is the one response
    /// an operator has to copy from.
    #[post("")]
    async fn create(&self, auth: ConsoleAuthed, Json(body): JsonBody<EndpointRequest>) -> Response {
        let state = current_state();
        self.service
            .create_endpoint(&state, &auth, body)
            .await
            .into_response()
    }

    /// Says how a delivery is signed, in the words the receiver is written from.
    #[get("/spec")]
    async fn spec(&self, _auth: ConsoleAuthed) -> Response {
        self.service.signing_spec().await.into_response()
    }

    #[put("/{endpoint_id}")]
    async fn update(
        &self,
        auth: ConsoleAuthed,
        Path(endpoint_id): Path<String>,
        Json(body): JsonBody<EndpointRequest>,
    ) -> Response {
        let state = current_state();
        self.service
            .update_endpoint(&state, &auth, &endpoint_id, body)
            .await
            .into_response()
    }

    #[delete("/{endpoint_id}")]
    async fn delete(&self, auth: ConsoleAuthed, Path(endpoint_id): Path<String>) -> Response {
        let state = current_state();
        self.service
            .delete_endpoint(&state, &auth, &endpoint_id)
            .await
            .into_response()
    }

    #[get("/{endpoint_id}/deliveries")]
    async fn deliveries(&self, _auth: ConsoleAuthed, Path(endpoint_id): Path<String>) -> Response {
        let state = current_state();
        self.service
            .list_deliveries(&state, &endpoint_id)
            .await
            .into_response()
    }

    #[post("/{endpoint_id}/deliveries/{delivery_id}/retry")]
    async fn retry(
        &self,
        auth: ConsoleAuthed,
        Path((endpoint_id, delivery_id)): Path<(String, String)>,
    ) -> Response {
        let state = current_state();
        self.service
            .retry_delivery(&state, &auth, &endpoint_id, &delivery_id)
            .await
            .into_response()
    }
}
