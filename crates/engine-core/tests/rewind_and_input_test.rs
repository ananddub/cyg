use engine_core::{
    FixedClock, Interpolation, PlayerInput, PlayerInputBuffer, RewindManager, TransformSnapshot,
};

#[test]
fn test_rewind_lag_compensation_interpolation() {
    println!("\n=== TEST: Rewind Lag Compensation Interpolation ===");

    // 500ms rewind window at 60Hz tick rate
    let mut rewind = RewindManager::new(500, 60);
    let entity_id = 42;

    // Entity moves along X axis:
    // Tick 1 (t = 0.0ms)   -> x = 100.0, y = 50.0, z = 0.0, radius = 2.0
    // Tick 2 (t = 50.0ms)  -> x = 150.0, y = 50.0, z = 0.0, radius = 2.0
    // Tick 3 (t = 100.0ms) -> x = 200.0, y = 50.0, z = 0.0, radius = 2.0
    rewind.record(
        entity_id,
        1,
        0.0,
        TransformSnapshot::new(100.0, 50.0, 0.0, 2.0),
    );
    rewind.record(
        entity_id,
        2,
        50.0,
        TransformSnapshot::new(150.0, 50.0, 0.0, 2.0),
    );
    rewind.record(
        entity_id,
        3,
        100.0,
        TransformSnapshot::new(200.0, 50.0, 0.0, 2.0),
    );

    // Case 1: Query exact sample at t = 50.0ms
    let pos_50 = rewind.rewind_entity(entity_id, 50.0).expect("Found sample");
    assert_eq!(pos_50.x, 150.0);
    println!("✓ Exact sample at t=50ms: x = {}", pos_50.x);

    // Case 2: Query interpolated time at t = 25.0ms (halfway between 0ms and 50ms)
    // Expected x = 100.0 + (150.0 - 100.0) * 0.5 = 125.0
    let pos_25 = rewind.rewind_entity(entity_id, 25.0).expect("Found sample");
    assert!((pos_25.x - 125.0).abs() < 0.001);
    println!(
        "✓ Interpolated at t=25ms: x = {} (Expected 125.0)",
        pos_25.x
    );

    // Case 3: Query interpolated time at t = 75.0ms (halfway between 50ms and 100ms)
    // Expected x = 150.0 + (200.0 - 150.0) * 0.5 = 175.0
    let pos_75 = rewind.rewind_entity(entity_id, 75.0).expect("Found sample");
    assert!((pos_75.x - 175.0).abs() < 0.001);
    println!(
        "✓ Interpolated at t=75ms: x = {} (Expected 175.0)",
        pos_75.x
    );

    // Case 4: Hit registration test
    // Player fires shot at (x=124.5, y=50.0, z=0.0) with hit radius 2.0
    assert!(pos_25.is_point_hit(124.5, 50.0, 0.0));
    // Far miss at (x=140.0)
    assert!(!pos_25.is_point_hit(140.0, 50.0, 0.0));
    println!("✓ Hitbox validation at rewound timestamp verified!");
}

#[test]
fn test_player_input_buffer_sequencing_and_reconciliation_ack() {
    println!("\n=== TEST: Player Input Buffer & Reconciliation Acks ===");

    let mut buffer = PlayerInputBuffer::new(64);

    // 1. Client sends inputs #1, #2, #3
    assert!(buffer.push(PlayerInput::new(1, 10.0, vec![1, 0])));
    assert!(buffer.push(PlayerInput::new(2, 26.6, vec![1, 0])));
    assert!(buffer.push(PlayerInput::new(3, 43.3, vec![0, 1])));

    // 2. Duplicate packet arriving via UDP/Network -> must be rejected
    assert!(!buffer.push(PlayerInput::new(2, 26.6, vec![1, 0])));

    // 3. Out-of-order packets arriving: client sent #5 then #4
    assert!(buffer.push(PlayerInput::new(5, 76.6, vec![0, 1])));
    assert!(buffer.push(PlayerInput::new(4, 60.0, vec![1, 1])));

    assert_eq!(buffer.pending_count(), 5);

    // 4. Server simulation consumes inputs for Tick 1
    let input_1 = buffer.consume().expect("Input 1");
    assert_eq!(input_1.seq, 1);
    assert_eq!(buffer.consumed_count(), 1);

    let input_2 = buffer.consume().expect("Input 2");
    assert_eq!(input_2.seq, 2);
    assert_eq!(buffer.consumed_count(), 2);

    println!(
        "✓ Consumed inputs #1 and #2. Current Ack = {}",
        buffer.consumed_count()
    );

    // 5. Late arrival of old input #1 -> must be rejected because server already processed up to #2
    assert!(!buffer.push(PlayerInput::new(1, 10.0, vec![1, 0])));

    // 6. Remaining inputs must be in perfect order (3, 4, 5) even though #5 was pushed before #4
    let remaining = buffer.consume_batch(10);
    assert_eq!(remaining.len(), 3);
    assert_eq!(remaining[0].seq, 3);
    assert_eq!(remaining[1].seq, 4);
    assert_eq!(remaining[2].seq, 5);

    assert_eq!(buffer.consumed_count(), 5);
    assert_eq!(buffer.last_consumed_seq(), 5);
    assert_eq!(buffer.pending_count(), 0);

    println!(
        "✓ Total reconciliation ack sent back to client: {}",
        buffer.consumed_count()
    );
}

#[test]
fn test_fixed_clock_ticks() {
    let mut clock = FixedClock::new(60);
    assert_eq!(clock.tick_rate, 60);

    let tick_1 = clock.advance();
    assert_eq!(tick_1.tick, 1);
    assert!((tick_1.dt - 0.016666668).abs() < 0.0001);

    let tick_2 = clock.advance();
    assert_eq!(tick_2.tick, 2);
}

#[test]
fn test_step_interpolation_holds_pre_teleport_value() {
    // Linear lerp would smear across a teleport jump; Step must hold the old value.
    let mut rewind = RewindManager::new_with_interpolation(500, 60, Interpolation::Step);
    // Teleports at t=50.0 from x=0 to x=1000.
    rewind.record(1, 1, 33.3, TransformSnapshot::new(0.0, 0.0, 0.0, 1.0));
    rewind.record(1, 2, 50.0, TransformSnapshot::new(0.0, 0.0, 0.0, 1.0));
    rewind.record(1, 3, 50.1, TransformSnapshot::new(1000.0, 0.0, 0.0, 1.0));
    rewind.record(1, 4, 66.6, TransformSnapshot::new(1000.0, 0.0, 0.0, 1.0));

    // Just before the teleport snap: must be exactly the old position (no smear).
    let at = rewind.rewind_entity(1, 50.05).expect("found");
    assert_eq!(at.x, 0.0, "Step must hold pre-teleport value");

    // After the snap: exactly the new position.
    let at = rewind.rewind_entity(1, 50.2).expect("found");
    assert_eq!(at.x, 1000.0, "Step must hold post-teleport value");
}

#[test]
fn test_fast_path_and_fallback_agree_on_same_timestamps() {
    // Interpolation must be identical whether the bounding pair is found by
    // the O(1) contiguous-tick fast path or the backward-scan fallback.
    // Both histories here share identical timestamps; only tick numbers differ
    // (contiguous ticks arm the fast path, sparse ticks force the fallback).
    let times = [0.0f64, 16.6, 33.2, 83.2, 99.8];
    let xs = [0.0f32, 10.0, 20.0, 30.0, 40.0];

    let mut fast = RewindManager::new(500, 60);
    let mut slow = RewindManager::new(500, 60);
    for (i, (&t, &x)) in times.iter().zip(xs.iter()).enumerate() {
        fast.record(7, i as u64 + 1, t, TransformSnapshot::new(x, 0.0, 0.0, 1.0));
        slow.record(
            7,
            (i as u64 + 1) * 100,
            t,
            TransformSnapshot::new(x, 0.0, 0.0, 1.0),
        );
    }

    for target in [5.0f64, 21.0, 44.5, 50.0, 70.0] {
        let a = fast.rewind_entity(7, target).unwrap().x;
        let b = slow.rewind_entity(7, target).unwrap().x;
        assert!(
            (a - b).abs() < 1e-4,
            "mismatch at {target}: fast={a} slow={b}"
        );
    }
}

#[test]
fn test_rewind_all_into_reuses_buffer() {
    let mut rewind = RewindManager::new(500, 60);
    for id in 0..3u64 {
        rewind.record(
            id,
            1,
            16.6,
            TransformSnapshot::new(id as f32, 0.0, 0.0, 1.0),
        );
    }
    let mut buf = Vec::new();
    rewind.rewind_all_into(16.6, &mut buf);
    assert_eq!(buf.len(), 3);
    let cap_before = buf.capacity();
    rewind.rewind_all_into(16.6, &mut buf);
    assert_eq!(
        buf.capacity(),
        cap_before,
        "buffer must be reused, not reallocated"
    );
}
