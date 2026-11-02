
use engine_core::{PlayerInput, PlayerInputBuffer, RewindManager, TransformSnapshot};

#[test]
fn rewind_survives_a_forward_tick_jump() {

    let mut mgr = RewindManager::new(60_000, 60);

    for tick in 0..40u64 {
        mgr.record(
            1,
            tick,
            tick as f64 * 16.666,
            TransformSnapshot::new(tick as f32, 0.0, 0.0, 1.0),
        );
    }
    for tick in 10_040..10_080u64 {
        mgr.record(
            1,
            tick,
            tick as f64 * 16.666,
            TransformSnapshot::new(999.0, 0.0, 0.0, 1.0),
        );
    }

    let snap = mgr.rewind_entity(1, 10_040.0 * 16.666).unwrap();
    assert_eq!(snap.x, 999.0, "post-jump sample must be reachable");
    let snap = mgr.rewind_entity(1, 39.0 * 16.666).unwrap();
    assert_eq!(snap.x, 39.0, "pre-jump sample must stay exact");

    let snap = mgr.rewind_entity(1, 5_000.0 * 16.666).unwrap();
    assert!(
        (39.0..=999.0).contains(&snap.x),
        "gap query must stay bounded between pre- and post-jump samples, got x={}",
        snap.x
    );
}

#[test]
fn rewind_evicts_across_a_jump_like_any_cadence() {

    let mut mgr = RewindManager::new(500, 60);
    for tick in 0..40u64 {
        mgr.record(
            1,
            tick,
            tick as f64 * 16.666,
            TransformSnapshot::new(tick as f32, 0.0, 0.0, 1.0),
        );
    }
    for tick in 10_040..10_080u64 {
        mgr.record(
            1,
            tick,
            tick as f64 * 16.666,
            TransformSnapshot::new(999.0, 0.0, 0.0, 1.0),
        );
    }
    let snap = mgr.rewind_entity(1, 5_000.0 * 16.666).unwrap();
    assert_eq!(
        snap.x, 999.0,
        "gap query clamps to the oldest retained (post-jump) sample"
    );
}

#[test]
fn input_buffer_survives_a_forward_seq_jump() {

    let mut buf = PlayerInputBuffer::new(16);
    for seq in 0..8u64 {
        assert!(buf.push(PlayerInput::new(seq, 0.0, [0u8; 16])));
    }
    for _ in 0..8 {
        buf.consume();
    }
    let acked = buf.consumed_count();

    let mut next = 10_000u64;
    for i in 0..8u64 {
        assert!(
            buf.push(PlayerInput::new(next + i, 0.0, [0u8; 16])),
            "post-jump input must be accepted"
        );
    }
    while let Some(input) = buf.consume() {
        assert!(input.seq >= 10_000, "post-jump inputs drain after the jump");
        assert_eq!(
            buf.consumed_count(),
            acked + 1 + (input.seq - 10_000) as u64,
            "ack counter continues monotonically across the jump"
        );
    }

    assert!(!buf.push(PlayerInput::new(4, 0.0, [0u8; 16])));
}

#[test]
fn rewind_rejects_backward_tick_jump_without_panic() {

    let mut mgr = RewindManager::new(500, 60);
    for tick in 0..40u64 {
        mgr.record(
            1,
            tick,
            tick as f64 * 16.666,
            TransformSnapshot::new(tick as f32, 0.0, 0.0, 1.0),
        );
    }
    for tick in 0..20u64 {
        mgr.record(
            1,
            tick,
            tick as f64 * 16.666,
            TransformSnapshot::new(-1.0, 0.0, 0.0, 1.0),
        );
    }

    let snap = mgr.rewind_entity(1, 39.0 * 16.666).unwrap();
    assert_eq!(snap.x, 39.0, "pre-jump newest sample must survive the rewind attempt");
}
