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
        "seq",
        "updated_ms",
    ] {
        assert!(map.contains_key(key), "missing wire key {key:?}");
    }
    assert_eq!(map.len(), 27);
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
