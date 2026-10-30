use axum::Json;
use axum::http::HeaderMap;
use axum::response::{IntoResponse, Response};
use serde_json::{Value, json};

use crate::ApiState;

use super::StatusService;
use crate::utils::status::{age_ms, client_host};

/// The dialing details are identical for every room this process hosts, so they
/// are built once and stamped onto each row.
fn plumbing(state: &ApiState, host: &str, age_ms: Option<u64>, tick_hz: f64) -> Value {
    json!({
        "maxClients": state.game.max_clients,
        "tickHz": tick_hz,
        "tickRate": state.game.tick_rate,
        "port": state.game.port,
        "controlPort": state.control_port(),
        "serverId": format!("http://{host}:{}", state.control_port()),
        "signalingUrl": format!("ws://{host}:{}/ws/signal", state.control_port()),
        "udpPort": std::env::var("OX_UDP_ADDR").unwrap_or_else(|_| "0.0.0.0:7778".into()),
        "ageMs": age_ms,
    })
}

fn with_plumbing(mut row: Value, plumbing: &Value) -> Value {
    let (Some(map), Some(shared)) = (row.as_object_mut(), plumbing.as_object()) else {
        return row;
    };
    for (key, value) in shared {
        map.insert(key.clone(), value.clone());
    }
    row
}

impl StatusService {
    /// One entry per room the host loop is actually running. Before the first
    /// sim sample there is nothing to report, so the reply stays the single
    /// game-named row a cold process has always returned.
    pub async fn rooms(&self, state: &ApiState, headers: HeaderMap) -> Response {
        let snapshot = state.latest_status.lock().ok().and_then(|g| g.clone());
        let players = snapshot.as_ref().map(|s| s.players).unwrap_or(0);
        let tick_hz = snapshot.as_ref().map(|s| s.tick_hz_est).unwrap_or(0.0);
        let max_clients = state.game.max_clients.max(0) as u32;
        let host = client_host(&headers);
        let age_ms = snapshot.as_ref().map(age_ms);
        let plumbing = plumbing(state, &host, age_ms, tick_hz);
        // A cold process has no room table and has not run a tick, so its phase
        // is genuinely unknown (`null`) rather than a guess.
        let fallback_phase = snapshot
            .as_ref()
            .map(|_| if players > 0 { "active" } else { "hibernating" });

        let rows = match snapshot.as_ref() {
            Some(snap) if !snap.rooms.is_empty() => snap
                .rooms
                .iter()
                .map(|room| {
                    with_plumbing(
                        json!({
                            "name": room.name,
                            "clients": room.clients,
                            "spectators": room.spectators,
                            "phase": room.phase,
                            "ready": room.clients < max_clients,
                            "tick": room.tick,
                            "ageSec": room.age_sec,
                            "closesInSec": room.closes_in_sec,
                            "hasSimSample": true,
                        }),
                        &plumbing,
                    )
                })
                .collect(),
            _ => vec![with_plumbing(
                json!({
                    "name": state.game.name,
                    "clients": players,
                    "spectators": 0,
                    "phase": fallback_phase.as_deref(),
                    "ready": players < max_clients,
                    "hasSimSample": snapshot.is_some(),
                }),
                &plumbing,
            )],
        };
        Json(rows).into_response()
    }
}

pub async fn rooms(state: &ApiState, headers: HeaderMap) -> Response {
    StatusService.rooms(state, headers).await
}
