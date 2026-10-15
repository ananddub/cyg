//! The upgrade gate: verify the seat token, then hand the socket to the relay.
//!
//! The seat token minted by [`crate::services::matchmake`] **is** the authentication: an
//! HMAC over `"<room>:<expiry>"`, so a caller that never did matchmake cannot open a session.

use axum::extract::ws::WebSocketUpgrade;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};

use crate::dto::signaling::SignalQuery;
use crate::infra::signaling::socket::run_socket;
use super::SignalingService;
use crate::ApiState;

impl SignalingService {
    /// `GET /ws/signal?seat=<seatToken>`: the seat check, then the upgrade.
    pub fn signal(&self, state: &ApiState, query: SignalQuery, ws: WebSocketUpgrade) -> Response {
        let seat = match query.seat.as_deref() {
            Some(token) => state.verify_seat(token),
            None => Err("a seat token from matchmake is required".to_string()),
        };
        match seat {
            // Resolved before the upgrade: an unauthenticated socket never
            // becomes a task that has to be cleaned up afterwards.
            Ok(seat) => {
                let state = state.clone();
                ws.on_upgrade(move |socket| run_socket(socket, state, seat))
            }
            Err(reason) => crate::ApiError::new(StatusCode::UNAUTHORIZED, reason).into_response(),
        }
    }
}
