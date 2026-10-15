use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};

use crate::ApiState;
use crate::seats::Seat;

/// `ISeatReservation` plus the data-plane fields. Field order is irrelevant to
/// `JSON.parse`; the values must be escaped, because `room` reached us through
/// a URL and is echoed straight back.
pub struct SeatResponse {
    pub name: String,
    pub session_id: String,
    pub room_id: String,
    pub process_id: String,
    pub reconnection_token: String,
    pub seat_token: String,
    pub game_port: u16,
    pub control_port: u16,
    pub ice_servers: String,
    pub expires_at: u64,
}

impl SeatResponse {
    pub(super) fn new(state: &ApiState, seat: &Seat) -> SeatResponse {
        SeatResponse {
            name: seat.room.clone(),
            session_id: seat.id.clone(),
            room_id: seat.room.clone(),
            process_id: format!("openoxide_{}", std::process::id()),
            reconnection_token: seat.token.clone(),
            seat_token: seat.token.clone(),
            game_port: state.game.port,
            control_port: state.control_port(),
            ice_servers: state.signals.ice_config().to_string(),
            expires_at: seat.expires_at,
        }
    }

    pub fn to_json(&self) -> String {
        format!(
            concat!(
                "{{\"name\":\"{name}\",\"sessionId\":\"{sid}\",\"roomId\":\"{room}\",",
                "\"processId\":\"{proc}\",\"reconnectionToken\":\"{token}\",",
                "\"seatToken\":\"{seat}\",\"gamePort\":{gport},\"controlPort\":{cport},",
                "\"expiresAt\":{exp},\"iceServers\":{ice}}}"
            ),
            name = json_escape(&self.name),
            sid = json_escape(&self.session_id),
            room = json_escape(&self.room_id),
            proc = json_escape(&self.process_id),
            token = json_escape(&self.reconnection_token),
            seat = json_escape(&self.seat_token),
            gport = self.game_port,
            cport = self.control_port,
            exp = self.expires_at,
            ice = json_string_literal(&self.ice_servers),
        )
    }
}

impl IntoResponse for SeatResponse {
    fn into_response(self) -> Response {
        (
            StatusCode::OK,
            [(axum::http::header::CONTENT_TYPE, "application/json")],
            self.to_json(),
        )
            .into_response()
    }
}

pub(super) fn json_escape(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for c in value.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

/// A JSON string literal for a value that is itself JSON text (the ICE server
/// blob), so the client parses the outer object and then the inner one.
fn json_string_literal(value: &str) -> String {
    format!("\"{}\"", json_escape(value))
}

/// Pull one string field out of a JSON body. Matchmake bodies are small and
/// flat, and a malformed one must be a 400 from the caller, not a panic here.
pub fn json_string_field(body: &str, key: &str) -> Option<String> {
    let value = serde_json::from_str::<serde_json::Value>(body).ok()?;
    value.get(key)?.as_str().map(str::to_string)
}
