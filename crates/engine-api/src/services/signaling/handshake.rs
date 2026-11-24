use axum::extract::ws::WebSocketUpgrade;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};

use super::SignalingService;
use crate::dto::signaling::SignalQuery;
use crate::infra::signaling::socket::run_socket;

impl SignalingService {
    pub fn signal(&self, query: SignalQuery, ws: WebSocketUpgrade) -> Response {
        let seat = match query.seat.as_deref() {
            Some(token) => self.plane.verify_seat(token),
            None => Err("a seat token from matchmake is required".to_string()),
        };
        match seat {
            Ok(seat) => {
                let plane = self.plane.clone();
                ws.on_upgrade(move |socket| run_socket(socket, plane, seat))
            }
            Err(reason) => crate::ApiError::new(StatusCode::UNAUTHORIZED, reason).into_response(),
        }
    }
}
