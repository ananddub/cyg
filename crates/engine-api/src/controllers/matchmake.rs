//! Auto-registered bindings for the seat routes.
//!
//! `auto_route` has no per-route layer, so the old `DefaultBodyLimit` 413 ceiling
//! is enforced here before the body is parsed — transport policy, not a service rule.

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

/// A matchmake body is a seat ticket, at most a few hundred bytes; the ceiling is
/// charged *after* the bucket, so a flood still costs only a hash lookup.
pub const MAX_BODY_BYTES: usize = 4096;

struct MatchmakeController {
    service: Arc<MatchmakeService>,
}

#[controller("")]
impl MatchmakeController {
    fn new(service: Arc<MatchmakeService>) -> Self {
        Self { service }
    }

    /// `/api/matchmake/…` is the documented path.
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

    /// `/matchmake/…` is the path the client already posts to.
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

/// Both spellings share one transport rule: cap the body, then reserve the seat.
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
    Ok(service.reserve(state, peer.ip(), &method, &room, &body)?.into_response())
}
