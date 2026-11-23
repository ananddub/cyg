use std::sync::Arc;

use auto_route::controller;
use axum::http::HeaderMap;
use axum::response::{IntoResponse, Response};

use crate::app::params::Body;
use crate::current_state;
use crate::dto::console::auth::{AuthenticateRequest, ConsoleModeResponse, LogoutRequest};
use crate::services::console::ConsoleService;

struct ConsoleAuthController {
    service: Arc<ConsoleService>,
}

#[controller("/v1/console/authenticate")]
impl ConsoleAuthController {
    fn new(service: Arc<ConsoleService>) -> Self {
        Self { service }
    }

    #[post("", response_body = crate::dto::console::auth::ConsoleSessionResponse)]
    async fn authenticate(&self, Body(body): Body<AuthenticateRequest>) -> Response {
        let state = current_state();
        self.service.authenticate(body).await.into_response()
    }

    /// `GET /v1/console/mode` — is this a local development session?
    ///
    /// The dashboard asks before it decides whether to render a login form.
    /// Deliberately unauthenticated: it reveals only whether `-d` was passed,
    /// which the process already announced on its own stdout.
    #[get("/mode", response_body = crate::dto::console::auth::ConsoleModeResponse)]
    async fn mode(&self) -> Response {
        let dev = current_state().dev_mode;
        axum::Json(ConsoleModeResponse { dev }).into_response()
    }

    #[post("/logout", response_body = crate::dto::Empty)]
    async fn logout(&self, headers: HeaderMap, Body(body): Body<LogoutRequest>) -> Response {
        let state = current_state();
        self.service.logout(&headers, body).await.into_response()
    }
}
