//! `/api/rooms`: the one room this process hosts, with live occupancy.

use axum::Json;
use axum::extract::State;
use axum::response::{IntoResponse, Response};
use serde_json::json;

use crate::ApiState;

use super::view::age_ms;

/// The host a client can actually dial.
///
/// `bind_addr` is what the *server* bound, and `0.0.0.0` is not connectable;
/// so echo the authority the caller used, re-pinned to the port this plane
/// really holds, and fall back to loopback when no `Host` arrived.
pub(super) fn client_host(headers: &axum::http::HeaderMap) -> String {
    let host = headers
        .get(axum::http::header::HOST)
        .and_then(|v| v.to_str().ok())
        .map(|authority| match authority.rsplit_once(':') {
            // A bare IPv6 host has no port and IPv6 literal ports follow a
            // `]`, so this only strips for IPv4/names.
            Some((h, p)) if !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()) && !h.contains(']') => h,
            _ => authority,
        })
        .unwrap_or("127.0.0.1");
    match host {
        "0.0.0.0" | "::" | "" | "*" => "127.0.0.1".to_string(),
        other => other.to_string(),
    }
}

pub(super) async fn rooms(State(state): State<ApiState>, headers: axum::http::HeaderMap) -> Response {
    let snapshot = state.latest_status.lock().ok().and_then(|g| g.clone());
    let players = snapshot.as_ref().map(|s| s.players).unwrap_or(0);
    let tick_hz = snapshot.as_ref().map(|s| s.tick_hz_est).unwrap_or(0.0);

    // `serverId` is what the SDK echoes back into `joinById`; a seat token is
    // not a room instance id, so this is an address, not a handle.
    let host = client_host(&headers);
    // `updated_ms` is monotonic, so an absent sample also means the *age* is
    // unknown: report `null` rather than a plausible-looking zero.
    let age_ms = snapshot.as_ref().map(age_ms);
    Json(json!([{
        "name": state.game.name,
        "clients": players,
        "maxClients": state.game.max_clients,
        "ready": players < state.game.max_clients.max(0) as u32,
        "tickHz": tick_hz,
        // The configured rate, so a client can tell an idle room from a sim
        // running slower than asked.
        "tickRate": state.game.tick_rate,
        "port": state.game.port,
        "controlPort": state.control_port(),
        "serverId": format!("http://{host}:{}", state.control_port()),
        // Port-qualified: a bare `ws://0.0.0.0/ws/signal` would send a browser
        // to port 80 and look like a dead server.
        "signalingUrl": format!("ws://{host}:{}/ws/signal", state.control_port()),
        "udpPort": std::env::var("OX_UDP_ADDR").unwrap_or_else(|_| "0.0.0.0:7778".into()),
        "hasSimSample": snapshot.is_some(),
        "ageMs": age_ms,
    }]))
    .into_response()
}
