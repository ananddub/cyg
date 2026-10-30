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

struct MetricsController;

#[controller("/metrics")]
impl MetricsController {
    fn new() -> Self {
        Self
    }

    #[get("")]
    async fn metrics(&self, headers: HeaderMap) -> Response {
        let state = current_state();
        if let Err(err) = crate::app::middleware::server_key_or_console(&headers, &state) {
            return err.into_response();
        }
        let sample = state
            .latest_status
            .lock()
            .ok()
            .and_then(|guard| guard.clone());
        let mut out = String::with_capacity(2048);
        let mut gauge = |name: &str, help: &str, value: String| {
            out.push_str(&format!(
                "# HELP {name} {help}\n# TYPE {name} gauge\n{name} {value}\n"
            ));
        };
        if let Some(snap) = sample {
            gauge(
                "cyg_tick_hz_est",
                "Measured tick rate (Hz)",
                format!("{:.2}", snap.tick_hz_est),
            );
            gauge(
                "cyg_nominal_tick_us",
                "Configured tick period (µs)",
                snap.nominal_tick_us.to_string(),
            );
            gauge(
                "cyg_tick_budget_p99_us",
                "Tick p99 wall time (µs)",
                snap.tick_budget_p99_us.to_string(),
            );
            gauge(
                "cyg_tick_overruns_total",
                "Ticks past their budget in the window",
                snap.tick_overruns.to_string(),
            );
            gauge(
                "cyg_desyncs_total",
                "Client checksum reports that disagreed with the host",
                snap.desyncs.to_string(),
            );
            gauge(
                "cyg_stage_sim_us",
                "Simulation-stage average µs/tick",
                snap.stage_sim_us.to_string(),
            );
            gauge(
                "cyg_kernel_to_sim_p50_us",
                "Kernel-to-sim p50 (µs)",
                snap.kernel_to_sim_p50_us.to_string(),
            );
            gauge(
                "cyg_kernel_to_sim_p99_us",
                "Kernel-to-sim p99 (µs)",
                snap.kernel_to_sim_p99_us.to_string(),
            );
            gauge("cyg_players", "Players in rooms", snap.players.to_string());
            gauge(
                "cyg_ws_sessions",
                "WebSocket sessions",
                snap.ws_sessions.to_string(),
            );
            gauge(
                "cyg_webrtc_sessions",
                "WebRTC sessions",
                snap.webrtc_sessions.to_string(),
            );
            gauge(
                "cyg_udp_sessions",
                "Native UDP sessions",
                snap.udp_sessions.to_string(),
            );
            gauge(
                "cyg_deltas_sent",
                "Deltas sent in the window",
                snap.deltas_sent.to_string(),
            );
            gauge(
                "cyg_deltas_dropped_full",
                "Deltas dropped (queue full)",
                snap.deltas_dropped_full.to_string(),
            );
            gauge(
                "cyg_inputs_dropped",
                "Inputs dropped",
                snap.inputs_dropped.to_string(),
            );
            let phase_count = |phase: engine_bus::RoomPhase| {
                snap.rooms.iter().filter(|row| row.phase == phase).count()
            };
            out.push_str(
                "# HELP cyg_rooms Live rooms by lifecycle phase\n# TYPE cyg_rooms gauge\n",
            );
            out.push_str(&format!(
                "cyg_rooms{{phase=\"active\"}} {}\n",
                phase_count(engine_bus::RoomPhase::Active)
            ));
            // Summed from the rows, not taken from `rooms.len() - active`: a room
            // that never seated anybody is `created`, not one that lost its last
            // player, and an autoscaler reads the two differently.
            out.push_str(&format!(
                "cyg_rooms{{phase=\"hibernating\"}} {}\n",
                phase_count(engine_bus::RoomPhase::Hibernating)
            ));
            out.push_str(&format!(
                "cyg_rooms{{phase=\"created\"}} {}\n",
                phase_count(engine_bus::RoomPhase::Created)
            ));
            out.push_str("# HELP cyg_tick_duration_us Tick wall time, current publish window\n# TYPE cyg_tick_duration_us histogram\n");
            let mut cumulative = 0u64;
            for (i, count) in snap.tick_histogram.iter().enumerate() {
                cumulative += u64::from(*count);
                let le = (1u64 << (i + 1)) - 1;
                out.push_str(&format!(
                    "cyg_tick_duration_us_bucket{{le=\"{le}\"}} {cumulative}\n"
                ));
            }
            out.push_str(&format!(
                "cyg_tick_duration_us_bucket{{le=\"+Inf\"}} {}\ncyg_tick_duration_us_count {}\n",
                snap.tick_samples, snap.tick_samples
            ));
        } else {
            gauge("cyg_tick_hz_est", "Measured tick rate (Hz)", "0".into());
            gauge(
                "cyg_nominal_tick_us",
                "Configured tick period (µs)",
                "0".into(),
            );
            gauge(
                "cyg_tick_budget_p99_us",
                "Tick p99 wall time (µs)",
                "0".into(),
            );
            gauge(
                "cyg_tick_overruns_total",
                "Ticks past their budget in the window",
                "0".into(),
            );
            gauge(
                "cyg_desyncs_total",
                "Client checksum reports that disagreed with the host",
                "0".into(),
            );
            gauge(
                "cyg_stage_sim_us",
                "Simulation-stage average µs/tick",
                "0".into(),
            );
            gauge(
                "cyg_kernel_to_sim_p50_us",
                "Kernel-to-sim p50 (µs)",
                "0".into(),
            );
            gauge(
                "cyg_kernel_to_sim_p99_us",
                "Kernel-to-sim p99 (µs)",
                "0".into(),
            );
            gauge("cyg_players", "Players in rooms", "0".into());
            gauge("cyg_ws_sessions", "WebSocket sessions", "0".into());
            gauge("cyg_webrtc_sessions", "WebRTC sessions", "0".into());
            gauge("cyg_udp_sessions", "Native UDP sessions", "0".into());
            gauge("cyg_deltas_sent", "Deltas sent in the window", "0".into());
            gauge(
                "cyg_deltas_dropped_full",
                "Deltas dropped (queue full)",
                "0".into(),
            );
            gauge("cyg_inputs_dropped", "Inputs dropped", "0".into());
            out.push_str(
                "# HELP cyg_rooms Live rooms by lifecycle phase\n# TYPE cyg_rooms gauge\n",
            );
            out.push_str("cyg_rooms{phase=\"active\"} 0\ncyg_rooms{phase=\"hibernating\"} 0\n");
            out.push_str("cyg_rooms{phase=\"created\"} 0\n");
            out.push_str("# HELP cyg_tick_duration_us Tick wall time, current publish window\n# TYPE cyg_tick_duration_us histogram\n");
            for i in 0..engine_bus::TICK_HISTOGRAM_BUCKETS {
                let le = (1u64 << (i + 1)) - 1;
                out.push_str(&format!("cyg_tick_duration_us_bucket{{le=\"{le}\"}} 0\n"));
            }
            out.push_str(
                "cyg_tick_duration_us_bucket{le=\"+Inf\"} 0\ncyg_tick_duration_us_count 0\n",
            );
        }
        (
            [(
                axum::http::header::CONTENT_TYPE,
                "text/plain; version=0.0.4",
            )],
            out,
        )
            .into_response()
    }
}
