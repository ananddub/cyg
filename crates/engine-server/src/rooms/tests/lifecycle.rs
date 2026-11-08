use super::*;

#[test]
fn the_room_machine_walks_membership_in_order() {
    let mut room = GameRoom::new_relay();
    room.note_state();
    assert_eq!(
        room.phase(),
        engine_bus::RoomPhase::Created,
        "a room that never seated anybody is new, not emptied"
    );

    room.join("sess_alice", 1.0, 1.0, b"");
    room.note_state();
    assert_eq!(room.phase(), engine_bus::RoomPhase::Active);

    room.handle_disconnect("sess_alice");
    room.note_state();
    assert_eq!(room.phase(), engine_bus::RoomPhase::Hibernating);

    room.join("sess_bob", 1.0, 1.0, b"");
    room.note_state();
    assert_eq!(
        room.phase(),
        engine_bus::RoomPhase::Active,
        "the next member wakes the same room, it is not a new one"
    );

    room.dispose();
    assert_eq!(room.phase(), engine_bus::RoomPhase::Closed);
    room.join("sess_carol", 1.0, 1.0, b"");
    room.note_state();
    assert_eq!(
        room.phase(),
        engine_bus::RoomPhase::Closed,
        "a disposed room has no way back on the machine"
    );
}

#[test]
fn only_an_active_room_is_simulated() {
    let gated_tick = |room: &mut GameRoom| {
        room.note_state();
        if room.phase() != engine_bus::RoomPhase::Active {
            return false;
        }
        room.tick(1.0 / 30.0, false).unwrap();
        true
    };

    let mut room = GameRoom::new_relay();
    assert!(!gated_tick(&mut room), "a Created room is not simulated");
    assert_eq!(room.current_tick, 0);

    room.join("sess_alice", 0.0, 0.0, b"");
    assert!(gated_tick(&mut room), "joining and ticking share one pass");
    assert_eq!(room.current_tick, 1);

    room.handle_disconnect("sess_alice");
    assert!(!gated_tick(&mut room), "Hibernating is 0% CPU");
    assert!(!gated_tick(&mut room));
    assert_eq!(room.current_tick, 1, "an emptied room ticks no further");
}

#[test]
fn close_reason_reports_which_lifecycle_window_expired() {
    let limited = engine_config::LifecycleConfig {
        room_max_seconds: 60,
        ..engine_config::LifecycleConfig::default()
    };
    let idle = engine_config::LifecycleConfig::default();

    let mut room = GameRoom::new_relay();
    room.join("sess_carol", 1.0, 1.0, b"");
    room.born = Instant::now() - Duration::from_secs(61);
    assert_eq!(
        room.close_reason(&limited, true, Instant::now()),
        Some(CloseReason::Duration)
    );
    assert_eq!(room.close_reason(&idle, true, Instant::now()), None);

    room.handle_disconnect("sess_carol");
    room.born = Instant::now();
    room.empty_since = Some(Instant::now() - Duration::from_secs(31));
    assert_eq!(
        room.close_reason(&idle, true, Instant::now()),
        Some(CloseReason::Empty)
    );
    assert_eq!(room.close_reason(&idle, false, Instant::now()), None);
}
