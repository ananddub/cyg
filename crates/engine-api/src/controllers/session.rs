//! Auto-registered bindings for `/v2/account/session/refresh` and `/v2/session/logout`.
//!
//! Methods return `Response` (not `Json<T>`) so `auto_route` does not require the
//! payload type to implement `poem_openapi`'s `Type`.

use std::sync::Arc;

use auto_route::controller;
use axum::http::HeaderMap;
use axum::response::{IntoResponse, Response};

use crate::app::params::Body;
use crate::current_state;

use crate::dto::session::{LogoutRequest, RefreshRequest};
use crate::services::session::SessionService;

struct SessionController {
    service: Arc<SessionService>,
}

#[controller("/v2")]
impl SessionController {
    fn new(service: Arc<SessionService>) -> Self {
        Self { service }
    }

    /// `POST /v2/account/session/refresh` — server key in, fresh token pair out.
    #[post("/account/session/refresh")]
    async fn refresh(&self, headers: HeaderMap, Body(req): Body<RefreshRequest>) -> Response {
        let state = current_state();
        self.service
            .refresh(&state, &headers, req)
            .await
            .into_response()
    }

    /// `POST /v2/session/logout` — bearer route that blacklists the named token halves.
    #[post("/session/logout")]
    async fn logout(&self, headers: HeaderMap, Body(req): Body<LogoutRequest>) -> Response {
        let state = current_state();
        self.service
            .logout(&state, &headers, req)
            .await
            .into_response()
    }
}
