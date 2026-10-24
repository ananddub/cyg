//! Auto-registered bindings for `/v2/console/authenticate*` — the one console
//! surface a caller reaches without a token.

use std::sync::Arc;

use auto_route::controller;
use axum::http::HeaderMap;
use axum::response::{IntoResponse, Response};

use crate::app::params::Body;
use crate::current_state;
use crate::dto::console::auth::{AuthenticateRequest, LogoutRequest};
use crate::services::console::ConsoleService;

struct ConsoleAuthController {
    service: Arc<ConsoleService>,
}

#[controller("/v2/console/authenticate")]
impl ConsoleAuthController {
    fn new(service: Arc<ConsoleService>) -> Self {
        Self { service }
    }

    #[post("")]
    async fn authenticate(&self, Body(body): Body<AuthenticateRequest>) -> Response {
        let state = current_state();
        self.service
            .authenticate(&state, body)
            .await
            .into_response()
    }

    #[post("/logout")]
    async fn logout(&self, headers: HeaderMap, Body(body): Body<LogoutRequest>) -> Response {
        let state = current_state();
        self.service
            .logout(&state, &headers, body)
            .await
            .into_response()
    }
}
