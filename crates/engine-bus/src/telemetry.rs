use serde::Serialize;

/// Bucket `i` of `tick_histogram` counts ticks in `[2^i, 2^(i+1))` µs; the
/// last bucket absorbs anything longer. 12 buckets reach 4.096 ms — one tick
/// budget at 128 Hz.
pub const TICK_HISTOGRAM_BUCKETS: usize = 12;

/// `transport` is `websocket` | `webrtc` | `native-udp`.
#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionTelemetry {
    pub session: String,
    pub transport: String,
    pub player_id: Option<i64>,
    /// As echoed in this session's delta tail. `None` if no input applied yet.
    pub acked_seq: Option<u32>,
    /// Since this session's last input; `None` if it never sent one.
    pub idle_ms: Option<u64>,
}

/// Sim → status sink: latest-wins snapshot for `/status`, `/api/dashboard/*`
/// and the log line. Field names are a wire contract.
#[derive(Clone, Debug, Default, Serialize)]
pub struct StatusSnapshot {
    pub tick_hz_est: f64,
    pub tick_budget_p50_us: u64,
    pub tick_budget_p99_us: u64,
    pub tick_overruns: u64,
    pub nominal_tick_us: u64,
    /// Simulation-stage share of the tick (room ticks incl. guest + physics +
    /// delta fan-out), average µs over the publish window. 0 = no samples yet.
    pub stage_sim_us: u64,
    /// Zero distinguishes "no samples yet" from "every tick was instant".
    pub tick_samples: u32,
    /// Reset by the producer every publish — describes the current window only.
    pub tick_histogram: [u32; TICK_HISTOGRAM_BUCKETS],
    pub kernel_to_sim_p50_us: u64,
    pub kernel_to_sim_p99_us: u64,
    pub inputs_dropped: u64,
    pub deltas_sent: u64,
    pub deltas_dropped_full: u64,
    pub players: u32,
    pub empty_for_sec: u64,
    /// WS→WebRTC upgrades are counted where packets arrive, so these need not
    /// sum to `players`.
    pub ws_sessions: u32,
    pub webrtc_sessions: u32,
    pub udp_sessions: u32,
    pub sessions: Vec<SessionTelemetry>,
    /// Per-producer counter; readers use it to detect gaps.
    pub seq: u64,
    /// [`crate::clock::mono_ns`] milliseconds — same epoch as the readers'.
    pub updated_ms: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

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
            "stage_sim_us",
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
        assert_eq!(map.len(), 21);
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
