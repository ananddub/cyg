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

    /// `GET /v2/console/mode` — is this a local development session?
    ///
    /// The dashboard asks before it decides whether to render a login form.
    /// Deliberately unauthenticated: it reveals only whether `-d` was passed,
    /// which the process already announced on its own stdout.
    #[get("/mode")]
    async fn mode(&self) -> Response {
        let dev = current_state().dev_mode;
        axum::Json(serde_json::json!({ "dev": dev })).into_response()
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
