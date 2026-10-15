use std::net::SocketAddr;

use axum::Router;
use axum::extract::{ConnectInfo, DefaultBodyLimit, Path as UrlPath, State};
use axum::response::{IntoResponse, Response};
use axum::routing::post;

use super::reserve::reserve;
use crate::ApiError;
use crate::ApiState;

/// The browser client's method whitelist. Anything else is a room handler the
/// engine does not have.
pub const MATCHMAKE_METHODS: &[&str] =
    &["joinOrCreate", "join", "joinById", "create", "rejoin", "reconnect"];

/// A matchmake body is a seat ticket, at most a few hundred bytes. Anything
/// larger is not this endpoint's traffic — and the ceiling is charged *after*
/// the bucket, so a flood still costs only a hash lookup.
pub const MAX_BODY_BYTES: usize = 4096;

/// Room names land in a URL, a log line, a JSON string and the sim plane's room
/// map. Keep them boring.
pub fn valid_room_name(room: &str) -> bool {
    (1..=64).contains(&room.len())
        && room
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

/// `pub fn router(&ApiState) -> Router<ApiState>` — frozen entry point.
///
/// `/matchmake/…` is the path the client already posts to; `/api/matchmake/…`
/// is the documented one. Both reach the same handler.
pub fn router(state: &ApiState) -> Router<ApiState> {
    let _ = state;
    Router::new()
        .route("/api/matchmake/{method}/{room}", post(handle))
        .route("/matchmake/{method}/{room}", post(handle))
        .layer(DefaultBodyLimit::max(MAX_BODY_BYTES))
}

async fn handle(
    State(state): State<ApiState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    UrlPath((method, room)): UrlPath<(String, String)>,
    body: String,
) -> Result<Response, ApiError> {
    Ok(reserve(&state, peer.ip(), &method, &room, &body)?.into_response())
}
