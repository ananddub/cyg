use std::sync::Arc;

use auto_route::controller;
use axum::response::{IntoResponse, Response};

use crate::app::middleware::Authed;
use crate::current_state;
use crate::services::flag::FlagService;

/// The experiments this account is in. A player is told which arm they were
/// placed in and nothing about the other arms, so the client can build the
/// branch it was assigned without guessing at the rest of the test.
struct FlagController {
    service: Arc<FlagService>,
}

#[controller("/v2/flags")]
impl FlagController {
    fn new(service: Arc<FlagService>) -> Self {
        Self { service }
    }

    #[get("")]
    async fn list(&self, auth: Authed) -> Response {
        let state = current_state();
        self.service
            .resolve(&state, auth.uid())
            .await
            .into_response()
    }
}
