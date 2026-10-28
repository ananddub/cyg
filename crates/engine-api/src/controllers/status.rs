//! Auto-registered controllers for the telemetry surface: `/status` and `/api/rooms`.
//!
//! The routes live in [`crate::services::status`]; this file only binds them to
//! paths through `auto_route`.

use std::sync::Arc;

use auto_route::controller;
use axum::http::HeaderMap;
use axum::response::{IntoResponse, Response};

use crate::current_state;

use crate::services::status::StatusService;

struct StatusController {
    service: Arc<StatusService>,
}

#[controller("/status")]
impl StatusController {
    fn new(service: Arc<StatusService>) -> Self {
        Self { service }
    }

    #[get("")]
    async fn status(&self) -> Response {
        let state = current_state();
        self.service.status(&state).await
    }
}

struct RoomsController {
    service: Arc<StatusService>,
}

#[controller("/api")]
impl RoomsController {
    fn new(service: Arc<StatusService>) -> Self {
        Self { service }
    }

    #[get("/rooms")]
    async fn rooms(&self, headers: HeaderMap) -> Response {
        let state = current_state();
        self.service.rooms(&state, headers).await
    }
}

/// Prometheus text exposition of the latest telemetry sample — the scrape
/// target fleet monitors point at. Same data `/status` returns, wire format
/// a Prometheus server ingests. Public like `/status`: the sample carries
/// aggregate gauges, no player-identifying fields.
struct MetricsController;

#[controller("/metrics")]
impl MetricsController {
    fn new() -> Self {
        Self
    }

    #[get("")]
    async fn metrics(&self) -> Response {
        let state = current_state();
        let sample = state
            .latest_status
            .lock()
            .ok()
            .and_then(|guard| guard.clone());
        let mut out = String::with_capacity(2048);
        let mut gauge = |name: &str, help: &str, value: String| {
            out.push_str(&format!("# HELP {name} {help}\n# TYPE {name} gauge\n{name} {value}\n"));
        };
        if let Some(snap) = sample {
            gauge("cyg_tick_hz_est", "Measured tick rate (Hz)", format!("{:.2}", snap.tick_hz_est));
            gauge("cyg_nominal_tick_us", "Configured tick period (µs)", snap.nominal_tick_us.to_string());
            gauge("cyg_tick_budget_p99_us", "Tick p99 wall time (µs)", snap.tick_budget_p99_us.to_string());
            gauge("cyg_tick_overruns_total", "Ticks past their budget in the window", snap.tick_overruns.to_string());
            gauge("cyg_stage_sim_us", "Simulation-stage average µs/tick", snap.stage_sim_us.to_string());
            gauge("cyg_players", "Players in rooms", snap.players.to_string());
            gauge("cyg_ws_sessions", "WebSocket sessions", snap.ws_sessions.to_string());
            gauge("cyg_webrtc_sessions", "WebRTC sessions", snap.webrtc_sessions.to_string());
            gauge("cyg_udp_sessions", "Native UDP sessions", snap.udp_sessions.to_string());
            gauge("cyg_deltas_sent", "Deltas sent in the window", snap.deltas_sent.to_string());
            gauge("cyg_deltas_dropped_full", "Deltas dropped (queue full)", snap.deltas_dropped_full.to_string());
            gauge("cyg_inputs_dropped", "Inputs dropped", snap.inputs_dropped.to_string());
        } else {
            // No sample yet — emit the families with zero so scrapers see a
            // stable schema from the first scrape.
            for name in [
                "cyg_tick_hz_est", "cyg_nominal_tick_us", "cyg_tick_budget_p99_us",
                "cyg_tick_overruns_total", "cyg_stage_sim_us", "cyg_players",
                "cyg_ws_sessions", "cyg_webrtc_sessions", "cyg_udp_sessions",
                "cyg_deltas_sent", "cyg_deltas_dropped_full", "cyg_inputs_dropped",
            ] {
                out.push_str(&format!("{name} 0\n"));
            }
        }
        (
            [(axum::http::header::CONTENT_TYPE, "text/plain; version=0.0.4")],
            out,
        )
            .into_response()
    }
}
