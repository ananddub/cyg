use super::*;
use crate::messages::{MatchFinished, SimEvent, Via};
use std::time::Instant;

fn input(seq: u16) -> InputMsg {
    InputMsg {
        player_id: 1,
        seq,
        payload: [0; 20],
        arrival_ns: 0,
        via: Via::NativeUdp,
    }
}

fn gameplay_event(seq: u64) -> SimEvent {
    SimEvent {
        room: "arena".into(),
        tick: seq as i64,
        seq,
        target: 1,
        event: "discard".into(),
        payload: vec![seq as u8],
    }
}

#[test]
fn push_input_accepts_until_full() {
    let bus = Bus::new(2, 2);
    bus.push_input(input(1));
    bus.push_input(input(2));
    assert_eq!(bus.inputs_rx.len(), 2);
}

#[test]
fn push_input_full_drops_the_oldest_and_keeps_the_newest() {
    let bus = Bus::new(2, 2);
    bus.push_input(input(1));
    bus.push_input(input(2));
    bus.push_input(input(3));
    assert_eq!(bus.inputs_rx.try_recv().unwrap().seq, 2);
    assert_eq!(bus.inputs_rx.try_recv().unwrap().seq, 3);
    assert!(bus.inputs_rx.try_recv().is_err());
}

#[test]
fn push_input_when_the_sim_is_gone_is_a_silent_noop() {
    let bus = Bus::new(2, 2);
    let tx = bus.inputs_tx.clone();
    let orphaned = Arc::try_unwrap(bus).ok().unwrap();
    drop(orphaned);
    assert!(matches!(
        tx.try_send(input(1)),
        Err(crossbeam_channel::TrySendError::Disconnected(_))
    ));
}

#[test]
fn an_event_reaches_the_reader_that_owns_the_database() {
    let bus = Bus::new(2, 2);
    bus.push_event(gameplay_event(7));
    let event = bus.events_rx.try_recv().expect("the event is queued");
    assert_eq!(event.room, "arena");
    assert_eq!(event.event, "discard");
    assert_eq!(event.seq, 7);
    assert_eq!(event.payload, vec![7]);
}

#[test]
fn a_full_event_history_queue_loses_the_oldest_row_not_the_newest() {
    let bus = Bus::new(2, 2);
    for seq in 1..=EVENT_CAP as u64 {
        bus.push_event(gameplay_event(seq));
    }
    assert_eq!(bus.events_rx.len(), EVENT_CAP);
    assert_eq!(bus.events_dropped(), 0);

    bus.push_event(gameplay_event(EVENT_CAP as u64 + 1));
    assert_eq!(bus.events_dropped(), 1, "the loss is counted, not silent");
    assert_eq!(
        bus.events_rx.try_recv().unwrap().seq,
        2,
        "seq 1 was discarded"
    );
    assert_eq!(
        // `try_iter`, not `iter`: the sender is still alive, so a blocking
        // iterator would wait for a disconnect that never comes.
        bus.events_rx.try_iter().last().unwrap().seq,
        EVENT_CAP as u64 + 1,
        "the fresh event is the one kept"
    );
}

#[test]
fn a_finished_match_is_queued_for_the_history_table() {
    let bus = Bus::new(2, 2);
    bus.push_match(MatchFinished {
        room: "arena".into(),
        game_id: "tank-3d".into(),
        end_reason: "duration".into(),
        started_at_unix: 1,
        ended_at_unix: 90,
        ticks: 2700,
        peak_players: 4,
        snapshot_seq: 3,
        result: b"{\"winner\":2}".to_vec(),
    });
    let row = bus.matches_rx.try_recv().expect("the match is queued");
    assert_eq!(row.end_reason, "duration");
    assert_eq!(row.ticks, 2700);
    assert_eq!(row.snapshot_seq, 3);
    assert_eq!(row.result, b"{\"winner\":2}".to_vec());
    assert_eq!(bus.matches_dropped(), 0);
}

#[test]
fn a_drain_request_latches_for_every_reader() {
    let bus = Bus::new(2, 2);
    assert!(!bus.is_draining());
    bus.request_drain();
    bus.request_drain();
    assert!(bus.is_draining());
    assert!(
        bus.clone().is_draining(),
        "the other plane sees the same latch"
    );
}

/// The control plane cannot tell a WASM save from an ECS save by looking at the
/// bytes, and a match row points at a save by its ordinal, so both ride along.
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

/// The fetch is the only bus call that waits, so its two failure modes have to
/// be answers rather than hangs: nobody pumping, and nobody holding the row.
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
    // The request left the queue, so the slot is free again for the next room.
    assert!(
        bus.fetches_rx.try_recv().is_err(),
        "the reader took the request"
    );
    answer.join().unwrap();
}

#[test]
fn an_unanswered_fetch_gives_up_and_starts_the_room_empty() {
    let bus = Bus::new(2, 2);
    // Nobody reads `fetches_rx`: a host with no control plane behind it boots a
    // room with no state, after waiting only as long as it said it would.
    let started = Instant::now();
    assert!(bus.fetch_snapshot("arena", Duration::from_millis(20)).is_none());
    assert!(
        started.elapsed() < Duration::from_secs(2),
        "the wait is bounded, not a hang"
    );

    // A `None` answer is a real one: the room never saved anything, and the
    // host must not retry it every time somebody asks for the room.
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
