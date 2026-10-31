use axum::Json;
use axum::http::HeaderMap;
use axum::response::{IntoResponse, Response};
use serde_json::json;

use crate::ApiState;

use super::StatusService;
use crate::utils::status::{age_ms, client_host};

impl StatusService {
    pub async fn rooms(&self, state: &ApiState, headers: HeaderMap) -> Response {
        let snapshot = state.latest_status.lock().ok().and_then(|g| g.clone());
        let players = snapshot.as_ref().map(|s| s.players).unwrap_or(0);
        let tick_hz = snapshot.as_ref().map(|s| s.tick_hz_est).unwrap_or(0.0);

        let host = client_host(&headers);
        let age_ms = snapshot.as_ref().map(age_ms);
        Json(json!([{
            "name": state.game.name,
            "clients": players,
            "maxClients": state.game.max_clients,
            "ready": players < state.game.max_clients.max(0) as u32,
            "tickHz": tick_hz,
            "tickRate": state.game.tick_rate,
            "port": state.game.port,
            "controlPort": state.control_port(),
            "serverId": format!("http://{host}:{}", state.control_port()),
            "signalingUrl": format!("ws://{host}:{}/ws/signal", state.control_port()),
            "udpPort": std::env::var("OX_UDP_ADDR").unwrap_or_else(|_| "0.0.0.0:7778".into()),
            "hasSimSample": snapshot.is_some(),
            "ageMs": age_ms,
        }]))
        .into_response()
    }
}

pub async fn rooms(state: &ApiState, headers: HeaderMap) -> Response {
    StatusService.rooms(state, headers).await
}
