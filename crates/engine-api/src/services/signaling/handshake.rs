use axum::extract::ws::WebSocketUpgrade;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};

use super::SignalingService;
use crate::ApiState;
use crate::dto::signaling::SignalQuery;
use crate::infra::signaling::socket::run_socket;

impl SignalingService {
    pub fn signal(&self, state: &ApiState, query: SignalQuery, ws: WebSocketUpgrade) -> Response {
        let seat = match query.seat.as_deref() {
            Some(token) => state.verify_seat(token),
            None => Err("a seat token from matchmake is required".to_string()),
        };
        match seat {
            Ok(seat) => {
                let state = state.clone();
                ws.on_upgrade(move |socket| run_socket(socket, state, seat))
            }
            Err(reason) => crate::ApiError::new(StatusCode::UNAUTHORIZED, reason).into_response(),
        }
    }
}
