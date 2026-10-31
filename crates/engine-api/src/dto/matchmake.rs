use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};

use crate::ApiState;
use crate::infra::seats::Seat;

pub struct SeatResponse {
    pub name: String,
    pub session_id: String,
    pub room_id: String,
    pub process_id: String,
    pub reconnection_token: String,
    pub seat_token: String,
    pub game_port: u16,
    pub control_port: u16,
    pub udp_port: u16,
    pub ice_servers: String,
    pub expires_at: u64,
}

impl SeatResponse {
    pub fn new(state: &ApiState, seat: &Seat) -> SeatResponse {
        let udp_port = std::env::var("OX_UDP_ADDR")
            .ok()
            .and_then(|addr| addr.split(':').last().and_then(|p| p.parse::<u16>().ok()))
            .unwrap_or(7778);

        SeatResponse {
            name: seat.room.clone(),
            session_id: seat.id.clone(),
            room_id: seat.room.clone(),
            process_id: format!("cyg_{}", std::process::id()),
            reconnection_token: seat.token.clone(),
            seat_token: seat.token.clone(),
            game_port: state.game.port,
            control_port: state.control_port(),
            udp_port,
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
                "\"udpPort\":{uport},\"expiresAt\":{exp},\"iceServers\":{ice}}}"
            ),
            name = json_escape(&self.name),
            sid = json_escape(&self.session_id),
            room = json_escape(&self.room_id),
            proc = json_escape(&self.process_id),
            token = json_escape(&self.reconnection_token),
            seat = json_escape(&self.seat_token),
            gport = self.game_port,
            cport = self.control_port,
            uport = self.udp_port,
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

pub fn json_escape(value: &str) -> String {
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

fn json_string_literal(value: &str) -> String {
    format!("\"{}\"", json_escape(value))
}

pub fn json_string_field(body: &str, key: &str) -> Option<String> {
    let value = serde_json::from_str::<serde_json::Value>(body).ok()?;
    value.get(key)?.as_str().map(str::to_string)
}
