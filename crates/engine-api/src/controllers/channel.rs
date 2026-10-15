//! Auto-registered bindings for `/v2/channel*`.
//!
//! Methods return `Response` (not `Json<T>`) so `auto_route` does not require
//! the payload type to implement `poem_openapi`'s `Type`.

use std::sync::Arc;

use auto_route::controller;
use axum::extract::Path;
use axum::response::{IntoResponse, Response};

use crate::current_state;
use crate::dto::channel::{ListQuery, SendRequest, UpdateRequest};
use crate::app::middleware::Authed;
use crate::app::params::{Body, Params};

use crate::services::channel::ChannelService;

struct ChannelController {
    service: Arc<ChannelService>,
}

#[controller("/v2/channel")]
impl ChannelController {
    fn new(service: Arc<ChannelService>) -> Self {
        Self { service }
    }

    #[get("/{channel_id}")]
    async fn list_messages(
        &self,
        auth: Authed,
        Path(channel_id): Path<String>,
        Params(req): Params<ListQuery>,
    ) -> Response {
        let state = current_state();
        self.service
            .list_messages(&state, &auth, &channel_id, req)
            .await
            .into_response()
    }

    #[post("/{channel_id}/message")]
    async fn send_message(
        &self,
        auth: Authed,
        Path(channel_id): Path<String>,
        Body(body): Body<SendRequest>,
    ) -> Response {
        let state = current_state();
        self.service
            .send_message(&state, &auth, &channel_id, body)
            .await
            .into_response()
    }

    #[put("/{channel_id}/message/{message_id}")]
    async fn update_message(
        &self,
        auth: Authed,
        Path((channel_id, message_id)): Path<(String, String)>,
        Body(body): Body<UpdateRequest>,
    ) -> Response {
        let state = current_state();
        self.service
            .update_message(&state, &auth, &channel_id, &message_id, body)
            .await
            .into_response()
    }

    #[delete("/{channel_id}/message/{message_id}")]
    async fn delete_message(
        &self,
        auth: Authed,
        Path((channel_id, message_id)): Path<(String, String)>,
    ) -> Response {
        let state = current_state();
        self.service
            .delete_message(&state, &auth, &channel_id, &message_id)
            .await
            .into_response()
    }
}
