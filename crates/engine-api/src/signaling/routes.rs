//! `/ws/signal`: the upgrade gate and the router entry.

use axum::Router;
use axum::extract::{Query, State};
use axum::extract::ws::WebSocketUpgrade;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use serde::Deserialize;

use crate::ApiState;

use super::socket::run_socket;

/// `pub fn router(&ApiState) -> Router<ApiState>` — frozen entry point.
pub fn router(_: &ApiState) -> Router<ApiState> {
    Router::new().route("/ws/signal", get(upgrade))
}

#[derive(Deserialize)]
pub struct SignalQuery {
    /// The `seatToken` from a matchmake reply. Required.
    seat: Option<String>,
}

async fn upgrade(
    State(state): State<ApiState>,
    Query(q): Query<SignalQuery>,
    ws: WebSocketUpgrade,
) -> Response {
    let seat = match q.seat.as_deref() {
        Some(token) => state.verify_seat(token),
        None => Err("a seat token from matchmake is required".to_string()),
    };
    match seat {
        // Resolved before the upgrade: an unauthenticated socket never becomes
        // a task that has to be cleaned up afterwards.
        Ok(seat) => ws.on_upgrade(move |socket| run_socket(socket, state, seat)),
        Err(reason) => crate::ApiError::new(StatusCode::UNAUTHORIZED, reason).into_response(),
    }
}
