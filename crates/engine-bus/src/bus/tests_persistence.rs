use super::*;
use std::time::Instant;

#[test]
fn a_snapshot_rides_the_queue_with_its_backend_and_ordinal() {
    let bus = Bus::new(2, 2);
    bus.push_snapshot(RoomSnapshot {
        room: "arena".into(),
        backend: "wasm".into(),
        seq: 2,
        bytes: vec![9],
    });
    let saved = bus.snapshots_rx.try_recv().expect("the snapshot is queued");
    assert_eq!(saved.room, "arena");
    assert_eq!(saved.backend, "wasm");
    assert_eq!(saved.seq, 2);
    assert_eq!(saved.bytes, vec![9]);
}

#[test]
fn a_snapshot_fetch_is_answered_by_the_control_plane() {
    let bus = Bus::new(2, 2);
    let reader = bus.clone();
    let answer = std::thread::spawn(move || {
        let fetch = reader.fetches_rx.recv().expect("a request arrives");
        assert_eq!(fetch.room, "arena");
        let _ = fetch.reply.send(Some(RoomSnapshot {
            room: fetch.room,
            backend: "wasm".into(),
            seq: 4,
            bytes: vec![1, 2],
        }));
    });
    let saved = bus
        .fetch_snapshot("arena", Duration::from_secs(5))
        .expect("the answer comes back on the reply");
    assert_eq!(saved.backend, "wasm");
    assert_eq!(saved.seq, 4);
    assert_eq!(saved.bytes, vec![1, 2]);

    assert!(
        bus.fetches_rx.try_recv().is_err(),
        "the reader took the request"
    );
    answer.join().unwrap();
}

#[test]
fn an_unanswered_fetch_gives_up_and_starts_the_room_empty() {
    let bus = Bus::new(2, 2);

    let started = Instant::now();
    assert!(
        bus.fetch_snapshot("arena", Duration::from_millis(20))
            .is_none()
    );
    assert!(
        started.elapsed() < Duration::from_secs(2),
        "the wait is bounded, not a hang"
    );

    let reader = bus.clone();
    std::thread::spawn(move || {
        let fetch = reader.fetches_rx.recv().unwrap();
        let _ = fetch.reply.send(None);
    });
    assert!(
        bus.fetch_snapshot("arena", Duration::from_secs(5))
            .is_none()
    );
}

#[test]
fn an_ownership_row_rides_the_queue_with_its_phase_and_ordinal() {
    let bus = Bus::new(2, 2);
    bus.push_ownership(crate::RoomOwnership {
        room: "arena".into(),
        node: "tank-3d-7f9c".into(),
        game: "tank-3d".into(),
        state: crate::RoomPhase::Active,
        peers: 3,
        tick: 1_200,
        snapshot_seq: 2,
    });

    let row = bus.ownership_rx.try_recv().expect("the claim is queued");
    assert_eq!(row.room, "arena");
    assert_eq!(row.node, "tank-3d-7f9c");
    assert_eq!(row.state, crate::RoomPhase::Active);
    assert_eq!(row.peers, 3);
    assert_eq!(row.tick, 1_200);
    assert_eq!(row.snapshot_seq, 2);
    assert_eq!(bus.ownership_dropped(), 0);
}

#[test]
fn a_full_ownership_queue_loses_the_oldest_beat_and_keeps_the_release() {
    let bus = Bus::new(2, 2);
    let row = |tick: i64| crate::RoomOwnership {
        room: "arena".into(),
        node: "tank-3d-7f9c".into(),
        game: "tank-3d".into(),
        state: crate::RoomPhase::Active,
        peers: 1,
        tick,
        snapshot_seq: 0,
    };
    for tick in 0..super::OWNERSHIP_CAP as i64 {
        bus.push_ownership(row(tick));
    }
    let mut release = row(super::OWNERSHIP_CAP as i64);
    release.state = crate::RoomPhase::Closed;
    bus.push_ownership(release);

    assert_eq!(bus.ownership_dropped(), 1, "one stale beat was discarded");
    let mut rows = Vec::new();
    while let Ok(row) = bus.ownership_rx.try_recv() {
        rows.push(row);
    }
    assert_eq!(rows.len(), super::OWNERSHIP_CAP);
    assert_eq!(rows[0].tick, 1, "the oldest beat went, the next one stayed");
    assert_eq!(rows[rows.len() - 1].state, crate::RoomPhase::Closed);
}

#[test]
fn a_seat_bridge_rides_the_queue_and_counts_its_losses() {
    let bus = Bus::new(2, 2);
    bus.push_seat(crate::SeatUpdate {
        room: "arena".into(),
        seat: 3,
        user_id: Some("0195c8da-1111-7000-8000-000000000001".into()),
    });

    let seat = bus.seats_rx.try_recv().expect("the bridge is queued");
    assert_eq!(seat.room, "arena");
    assert_eq!(seat.seat, 3);
    assert_eq!(
        seat.user_id.as_deref(),
        Some("0195c8da-1111-7000-8000-000000000001")
    );
    assert_eq!(bus.seats_dropped(), 0);

    for seat_id in 0..super::SEAT_CAP as i64 {
        bus.push_seat(crate::SeatUpdate {
            room: "arena".into(),
            seat: seat_id,
            user_id: None,
        });
    }
    bus.push_seat(crate::SeatUpdate {
        room: "arena".into(),
        seat: 9,
        user_id: None,
    });
    assert_eq!(bus.seats_dropped(), 1);
    let first = bus.seats_rx.try_recv().expect("a bridge survived");
    assert_eq!(first.seat, 1, "seat 0 was the one dropped");
}
