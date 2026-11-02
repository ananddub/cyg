use engine_core::{Interpolation, RewindManager, TransformSnapshot};

#[test]
fn test_rewind_lag_compensation_interpolation() {
    println!("\n=== TEST: Rewind Lag Compensation Interpolation ===");

    let mut rewind = RewindManager::new(500, 60);
    let entity_id = 42;

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

    let pos_50 = rewind.rewind_entity(entity_id, 50.0).expect("Found sample");
    assert_eq!(pos_50.x, 150.0);

    let pos_25 = rewind.rewind_entity(entity_id, 25.0).expect("Found sample");
    assert!((pos_25.x - 125.0).abs() < 0.001);

    let pos_75 = rewind.rewind_entity(entity_id, 75.0).expect("Found sample");
    assert!((pos_75.x - 175.0).abs() < 0.001);

    assert!(pos_25.is_point_hit(124.5, 50.0, 0.0));
    assert!(!pos_25.is_point_hit(140.0, 50.0, 0.0));
}

#[test]
fn test_step_interpolation_holds_pre_teleport_value() {
    let mut rewind = RewindManager::new_with_interpolation(500, 60, Interpolation::Step);

    rewind.record(1, 1, 33.3, TransformSnapshot::new(0.0, 0.0, 0.0, 1.0));
    rewind.record(1, 2, 50.0, TransformSnapshot::new(0.0, 0.0, 0.0, 1.0));
    rewind.record(1, 3, 50.1, TransformSnapshot::new(1000.0, 0.0, 0.0, 1.0));
    rewind.record(1, 4, 66.6, TransformSnapshot::new(1000.0, 0.0, 0.0, 1.0));

    let at = rewind.rewind_entity(1, 50.05).expect("found");
    assert_eq!(at.x, 0.0, "Step must hold pre-teleport value");

    let at = rewind.rewind_entity(1, 50.2).expect("found");
    assert_eq!(at.x, 1000.0, "Step must hold post-teleport value");
}

#[test]
fn test_fast_path_and_fallback_agree_on_same_timestamps() {
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
