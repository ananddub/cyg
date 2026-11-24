use super::*;
#[test]
fn guest_and_engine_rows_share_one_ordinal() {
    let bus = Bus::new(8, 8);
    let record = engine_config::GameplayConfig {
        record_events: vec!["deal".into()],
        ..Default::default()
    };
    let history = EventHistory::new(&bus, &record);

    history.mirror("arena", 1, &message("deal"));
    history.record_desync(
        "arena",
        0,
        &engine_wasm::DesyncReport {
            tick: 2,
            expected_checksum: 1,
            client_checksum: 2,
        },
    );
    history.mirror("arena", 3, &message("deal"));

    let seqs = [
        bus.events_rx.try_recv().unwrap().seq,
        bus.events_rx.try_recv().unwrap().seq,
        bus.events_rx.try_recv().unwrap().seq,
    ];
    assert_eq!(seqs, [1, 2, 3], "a reader must see no gap and no reuse");
}

#[test]
fn the_finished_match_keeps_the_last_result_and_the_snapshot_ordinal() {
    let bus = Bus::new(8, 8);
    let mut record = engine_config::GameplayConfig {
        result_event: "match_outcome".into(),
        ..Default::default()
    };
    record.normalize();
    let history = EventHistory::new(&bus, &record);
    let mut room = GameRoom::new_relay();
    room.snapshot_seq = 3;
    for (event, payload) in [
        ("chat", vec![7]),
        ("match_outcome", vec![1]),
        ("match_outcome", vec![2]),
    ] {
        room.ecs_outbox.push(OutboundMessage {
            target: 0,
            event: event.into(),
            payload: payload.into(),
        });
    }

    let mut transport = LoopTransport::new(
        WebRtcTransport::new("127.0.0.1:0".parse().unwrap()).unwrap(),
        None,
        None,
    );
    flush_room_outbox(&mut room, &mut transport, "arena", &history);
    assert_eq!(
        room.result,
        Some(bytes::Bytes::from_static(&[2])),
        "the last outcome stands"
    );

    report_match(&bus, "tank-3d", &room, "arena", "empty", 1);
    let row = bus.matches_rx.try_recv().expect("the match is queued");
    assert_eq!(row.result, vec![2]);
    assert_eq!(
        row.snapshot_seq, 3,
        "the match points at the room's last save"
    );
}

#[test]
fn the_history_row_names_every_way_a_match_can_end() {
    assert_eq!(CloseReason::Empty.label(), "empty");
    assert_eq!(CloseReason::Duration.label(), "duration");
    assert_eq!(DRAIN_REASON, "drain");
}

#[test]
fn a_persistent_or_udp_held_room_is_not_reapable() {
    let mut cfg = engine_config::GameConfig::default();
    cfg.name = "arena".into();
    cfg.lifecycle.persistent_rooms = vec!["lobby".into()];

    assert!(
        !room_is_reapable(&cfg, "lobby", 0),
        "a persistent room is never reaped"
    );
    assert!(
        !room_is_reapable(&cfg, "arena", 2),
        "the default room holds its seats while UDP peers remain"
    );
    assert!(
        room_is_reapable(&cfg, "arena", 0),
        "with no peers left it is a room like any other"
    );
    assert!(
        !room_is_reapable(&cfg, "side", 3),
        "a second room keeps its own UDP peers just like the default room"
    );
}
