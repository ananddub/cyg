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

/// Where one room sits in its lifecycle. A disposed room leaves the host's map
/// and is not reported at all, so `Hibernating` — empty, idle window running,
/// 0% CPU — is as close to `Closed` as a live room gets.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum RoomPhase {
    /// Members or spectators are attached; the room ticks.
    #[default]
    Active,
    /// Empty for `[lifecycle] reap_idle_secs`, its idle window counting down.
    Hibernating,
}

/// One row of the room table: the host's view of a single live room.
#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RoomTelemetry {
    pub name: String,
    pub phase: RoomPhase,
    pub clients: u32,
    pub spectators: u32,
    pub tick: u64,
    /// Seconds since the room was created.
    pub age_sec: u64,
    /// Seconds before `[lifecycle] room_max_seconds` closes it; `None` when the
    /// room length is unlimited, so a dashboard cannot render a fake countdown.
    pub closes_in_sec: Option<u64>,
}

/// Sim → status sink: latest-wins snapshot for `/status`, `/api/dashboard/*`
/// and the log line. Field names are a wire contract.
#[derive(Clone, Debug, Default, Serialize)]
pub struct StatusSnapshot {
    pub tick_hz_est: f64,
    pub tick_budget_p50_us: u64,
    pub tick_budget_p99_us: u64,
    pub tick_overruns: u64,
    /// Ticks where a client's checksum report disagreed with the host
    /// checkpoint. Non-zero means prediction has diverged somewhere; the
    /// `desync_report` rows carry the two checksums that prove it.
    pub desyncs: u64,
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
    /// One row per live room, so the control plane can see which rooms are
    /// hibernating instead of guessing from the aggregate player count.
    pub rooms: Vec<RoomTelemetry>,
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
            "desyncs",
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
            "rooms",
            "seq",
            "updated_ms",
        ] {
            assert!(map.contains_key(key), "missing wire key {key:?}");
        }
        assert_eq!(map.len(), 23);
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

    #[test]
    fn room_telemetry_names_its_phase_on_the_wire() {
        assert_eq!(
            serde_json::to_value(RoomPhase::default()).unwrap(),
            serde_json::json!("active")
        );
        let row = RoomTelemetry {
            name: "lobby".into(),
            phase: RoomPhase::Hibernating,
            clients: 0,
            spectators: 1,
            tick: 4_200,
            age_sec: 90,
            closes_in_sec: Some(30),
        };
        let value = serde_json::to_value(&row).unwrap();
        assert_eq!(value["phase"], serde_json::json!("hibernating"));
        for key in [
            "name",
            "phase",
            "clients",
            "spectators",
            "tick",
            "ageSec",
            "closesInSec",
        ] {
            assert!(
                value.as_object().unwrap().contains_key(key),
                "missing key {key:?}"
            );
        }
    }
}
