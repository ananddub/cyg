//! Telemetry types published by the sim once a second and read latest-wins
//! by `/status` and the dashboard API.
//!
//! Field names are a wire contract: `StatusSnapshot` serializes snake_case,
//! `SessionTelemetry` camelCase.

use serde::Serialize;

/// Bucket `i` of [`StatusSnapshot::tick_histogram`] counts ticks that ran
/// `[2^i, 2^(i+1))` microseconds; the last bucket absorbs anything longer.
/// 12 buckets reach 4.096 ms — one tick's budget at 128 Hz.
pub const TICK_HISTOGRAM_BUCKETS: usize = 12;

/// One connected client, as the host loop saw it at publish time.
/// `transport` is `websocket` | `webrtc` | `native-udp`.
#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionTelemetry {
    pub session: String,
    pub transport: String,
    pub player_id: Option<i64>,
    /// Highest input seq applied from this session, as echoed in its delta tail.
    pub acked_seq: Option<u32>,
    /// Milliseconds since this session's last input; `None` if it never sent one.
    pub idle_ms: Option<u64>,
}

/// Sim → status sink: latest-wins snapshot for `/status`, `/api/dashboard/*`
/// and the log line.
#[derive(Clone, Debug, Default, Serialize)]
pub struct StatusSnapshot {
    pub tick_hz_est: f64,
    pub tick_budget_p50_us: u64,
    pub tick_budget_p99_us: u64,
    pub tick_overruns: u64,
    /// Nominal tick period at the configured rate; p99 against this answers
    /// "are we keeping up".
    pub nominal_tick_us: u64,
    /// Ticks measured in this publish window; zero distinguishes "no samples
    /// yet" from "every tick was instant".
    pub tick_samples: u32,
    /// Base-2 histogram of tick durations, reset by the producer every publish.
    pub tick_histogram: [u32; TICK_HISTOGRAM_BUCKETS],
    pub kernel_to_sim_p50_us: u64,
    pub kernel_to_sim_p99_us: u64,
    pub inputs_dropped: u64,
    pub deltas_sent: u64,
    pub deltas_dropped_full: u64,
    pub players: u32,
    /// Seconds the room has been empty.
    pub empty_for_sec: u64,
    /// Sessions per transport; a session that upgraded WS → WebRTC is counted
    /// where its packets actually arrive, so these need not sum to `players`.
    pub ws_sessions: u32,
    pub webrtc_sessions: u32,
    pub udp_sessions: u32,
    /// Per-session rows, capped at the room's `max_clients`. Empty means the
    /// host published nothing, not that nobody is connected.
    pub sessions: Vec<SessionTelemetry>,
    /// Monotonic per-producer publish counter; readers use it to detect gaps.
    pub seq: u64,
    /// `mono_ns()` milliseconds at publish time, same process-wide epoch as
    /// the control plane's clock, which is how readers compute sample age.
    pub updated_ms: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Pins the exact snake_case key spelling `/status`, the dashboard API and
    /// `dashboard_contract.cjs` depend on.
    #[test]
    fn status_snapshot_wire_keys_are_pinned() {
        let value = serde_json::to_value(StatusSnapshot::default()).unwrap();
        let map = value.as_object().unwrap();
        for key in [
            "tick_hz_est",
            "tick_budget_p50_us",
            "tick_budget_p99_us",
            "tick_overruns",
            "nominal_tick_us",
            "tick_samples",
            "tick_histogram",
            "kernel_to_sim_p50_us",
            "kernel_to_sim_p99_us",
            "inputs_dropped",
            "deltas_sent",
            "deltas_dropped_full",
            "players",
            "empty_for_sec",
            "ws_sessions",
            "webrtc_sessions",
            "udp_sessions",
            "sessions",
            "seq",
            "updated_ms",
        ] {
            assert!(map.contains_key(key), "missing wire key {key:?}");
        }
        assert_eq!(map.len(), 20);
        assert_eq!(
            map["tick_histogram"].as_array().unwrap().len(),
            TICK_HISTOGRAM_BUCKETS
        );
    }

    #[test]
    fn session_telemetry_serializes_camel_case() {
        let value = serde_json::to_value(SessionTelemetry::default()).unwrap();
        let map = value.as_object().unwrap();
        for key in ["session", "transport", "playerId", "ackedSeq", "idleMs"] {
            assert!(map.contains_key(key), "missing key {key:?}");
        }
    }
}
