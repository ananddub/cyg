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
        "stage_guest_us",
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
        "rooms_created",
        "rooms_active",
        "rooms_hibernating",
        "server_phase",
        "seq",
        "updated_ms",
    ] {
        assert!(map.contains_key(key), "missing wire key {key:?}");
    }
    assert_eq!(map.len(), 28);
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
fn the_server_phase_is_pinned_and_terminal_at_shutdown() {
    let default = serde_json::to_value(ServerPhase::default()).unwrap();
    assert_eq!(default, serde_json::json!("booting"));
    for phase in [
        ServerPhase::Booting,
        ServerPhase::Ready,
        ServerPhase::Allocated,
        ServerPhase::Draining,
    ] {
        assert!(
            phase.allows(ServerPhase::Shutdown),
            "{phase:?} can shut down"
        );
        assert_eq!(phase.display(), phase.as_str());
    }
    assert!(!ServerPhase::Shutdown.allows(ServerPhase::Ready));
    assert!(ServerPhase::Ready.is_serving());
    assert!(ServerPhase::Allocated.is_serving());
    assert!(!ServerPhase::Booting.is_serving());
}

#[test]
fn the_room_machine_walks_forward_and_never_back() {
    assert!(RoomPhase::Created.allows(RoomPhase::Active));
    assert!(RoomPhase::Active.allows(RoomPhase::Hibernating));
    assert!(RoomPhase::Hibernating.allows(RoomPhase::Active));
    for state in [
        RoomPhase::Created,
        RoomPhase::Active,
        RoomPhase::Hibernating,
    ] {
        assert!(state.allows(RoomPhase::Closed), "{state:?} can be disposed");
    }
    assert!(
        !RoomPhase::Hibernating.allows(RoomPhase::Created),
        "an emptied room is not a new one"
    );
    assert!(!RoomPhase::Created.allows(RoomPhase::Hibernating));
    assert!(
        !RoomPhase::Closed.allows(RoomPhase::Active),
        "a disposed room does not come back"
    );

    for (state, name) in [
        (RoomPhase::Created, "created"),
        (RoomPhase::Active, "active"),
        (RoomPhase::Hibernating, "hibernating"),
        (RoomPhase::Closed, "closed"),
    ] {
        assert_eq!(
            serde_json::to_value(state).unwrap(),
            serde_json::json!(name)
        );
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
        room_id: "lobby".into(),
        phase: RoomPhase::Hibernating,
        clients: 0,
        spectators: 1,
        tick: 4_200,
        age_sec: 90,
        closes_in_sec: Some(30),
        max_clients: 16,
        locked: false,
        metadata: serde_json::Value::Null,
    };
    let value = serde_json::to_value(&row).unwrap();
    assert_eq!(value["phase"], serde_json::json!("hibernating"));
    for key in [
        "name",
        "roomId",
        "phase",
        "clients",
        "spectators",
        "tick",
        "ageSec",
        "closesInSec",
        "maxClients",
        "locked",
        "metadata",
    ] {
        assert!(
            value.as_object().unwrap().contains_key(key),
            "missing key {key:?}"
        );
    }
}
