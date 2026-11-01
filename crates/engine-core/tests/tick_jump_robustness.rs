//! Tick-jump robustness (Lightyear's `test_component_update_survives_a_clock_jump`
//! analogue): real servers hit clock discontinuities — NTP corrections, a
//! hibernated room resuming, a room migrated mid-match. A 10,000-tick jump
//! forward must not corrupt the input buffer's ordering/ack invariants or the
//! rewind ring's clamping, and a backward jump must be rejected cleanly.

use engine_core::{PlayerInput, PlayerInputBuffer, RewindManager, TransformSnapshot};

#[test]
fn rewind_survives_a_forward_tick_jump() {
    // Large window so pre-jump samples stay in the ring (500ms would evict
    // them — eviction semantics are pinned by the small-window variant below).
    let mut mgr = RewindManager::new(60_000, 60);
    // Normal cadence, then the clock jumps 10,000 ticks forward.
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
    // Both sides of the jump are queryable at their own times.
    let snap = mgr.rewind_entity(1, 10_040.0 * 16.666).unwrap();
    assert_eq!(snap.x, 999.0, "post-jump sample must be reachable");
    let snap = mgr.rewind_entity(1, 39.0 * 16.666).unwrap();
    assert_eq!(snap.x, 39.0, "pre-jump sample must stay exact");
    // A query inside the skipped gap lands between the bracketing samples
    // (the ring lerps across the discontinuity — no gameplay query should
    // target a 167-second gap, but it must stay bounded, never panic).
    let snap = mgr.rewind_entity(1, 5_000.0 * 16.666).unwrap();
    assert!(
        (39.0..=999.0).contains(&snap.x),
        "gap query must stay bounded between pre- and post-jump samples, got x={}",
        snap.x
    );
}

#[test]
fn rewind_evicts_across_a_jump_like_any_cadence() {
    // With the production 500ms window, a 10,000-tick jump evicts every
    // pre-jump sample (34-sample ring). Queries into the gap then clamp to
    // the oldest RETAINED (post-jump) sample — the ring treats the jump
    // exactly like fast-forwarding, which is the honest behavior.
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
    // A client that resyncs its own sequence numbering (reconnect, tick-based
    // seq) may jump its seq by thousands. The buffer must keep ordering and
    // ack invariants across the jump.
    let mut buf = PlayerInputBuffer::new(16);
    for seq in 0..8u64 {
        assert!(buf.push(PlayerInput::new(seq, 0.0, [0u8; 16])));
    }
    for _ in 0..8 {
        buf.consume();
    }
    let acked = buf.consumed_count();

    // +10,000 jump: accepted, ordered, ack continues monotonically.
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
    // And a stale pre-jump input is still rejected (already consumed).
    assert!(!buf.push(PlayerInput::new(4, 0.0, [0u8; 16])));
}

#[test]
fn rewind_rejects_backward_tick_jump_without_panic() {
    // A room whose clock moves backwards (restore from snapshot, replay)
    // must not corrupt the ring: the pre-jump samples stay queryable, the
    // backwards samples are discarded (monotonic enforcement).
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
    // The newest sample is still tick 39's; rewind to it works.
    let snap = mgr.rewind_entity(1, 39.0 * 16.666).unwrap();
    assert_eq!(snap.x, 39.0, "pre-jump newest sample must survive the rewind attempt");
}
