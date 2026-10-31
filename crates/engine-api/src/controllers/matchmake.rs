use std::net::SocketAddr;
use std::sync::Arc;

use auto_route::controller;
use axum::extract::{ConnectInfo, Path as UrlPath};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};

use crate::ApiError;
use crate::ApiState;
use crate::current_state;

use crate::services::matchmake::MatchmakeService;

pub const MAX_BODY_BYTES: usize = 4096;

struct MatchmakeController {
    service: Arc<MatchmakeService>,
}

#[controller("")]
impl MatchmakeController {
    fn new(service: Arc<MatchmakeService>) -> Self {
        Self { service }
    }

    #[post("/api/matchmake/{method}/{room}")]
    async fn api_matchmake(
        &self,
        peer: ConnectInfo<SocketAddr>,
        path: UrlPath<(String, String)>,
        body: String,
    ) -> Result<Response, ApiError> {
        let state = current_state();
        handle(&self.service, &state, peer, path, body)
    }

    #[post("/matchmake/{method}/{room}")]
    async fn matchmake(
        &self,
        peer: ConnectInfo<SocketAddr>,
        path: UrlPath<(String, String)>,
        body: String,
    ) -> Result<Response, ApiError> {
        let state = current_state();
        handle(&self.service, &state, peer, path, body)
    }
}

fn handle(
    service: &MatchmakeService,
    state: &ApiState,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    UrlPath((method, room)): UrlPath<(String, String)>,
    body: String,
) -> Result<Response, ApiError> {
    if body.len() > MAX_BODY_BYTES {
        return Ok(StatusCode::PAYLOAD_TOO_LARGE.into_response());
    }
    Ok(service
        .reserve(state, peer.ip(), &method, &room, &body)?
        .into_response())
}
