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

    #[post("/account/session/refresh")]
    async fn refresh(&self, headers: HeaderMap, Body(req): Body<RefreshRequest>) -> Response {
        let state = current_state();
        self.service
            .refresh(&state, &headers, req)
            .await
            .into_response()
    }

    #[post("/session/logout")]
    async fn logout(&self, headers: HeaderMap, Body(req): Body<LogoutRequest>) -> Response {
        let state = current_state();
        self.service
            .logout(&state, &headers, req)
            .await
            .into_response()
    }
}
