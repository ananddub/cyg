use engine_core::*;

#[test]
fn rewind_capacity_matches_window_formula() {
    let mut mgr = RewindManager::new(500, 60);
    for i in 0..40u64 {
        mgr.record(
            7,
            i,
            i as f64 * 16.666,
            TransformSnapshot::new(i as f32, 0.0, 0.0, 1.0),
        );
    }
    assert_eq!(mgr.len(), 1);

    let snap = mgr.rewind_entity(7, 0.0).unwrap();
    assert!(snap.x >= 6.0, "oldest retained x, got {}", snap.x);
    assert!(snap.x <= 8.0);
}

#[test]
fn rewind_ring_evicts_oldest_and_clamps_to_new_oldest() {
    let mut hist: EntityHistory<f64> = EntityHistory::new(3);
    for i in 0..6u64 {
        hist.record(i, i as f64 * 100.0, i as f64);
    }
    assert_eq!(hist.len(), 3);

    let (prev, next, _) = hist.find_samples(350.0).unwrap();
    assert_eq!(prev, 3.0);
    assert_eq!(next, 4.0);

    let (prev, _, _) = hist.find_samples(0.0).unwrap();
    assert_eq!(prev, 3.0);

    let (prev, next, _) = hist.find_samples(99_999.0).unwrap();
    assert_eq!(prev, 5.0);
    assert_eq!(next, 5.0);
}

#[test]
fn rewind_unknown_entity_and_remove_are_none() {
    let mut mgr = RewindManager::new(500, 60);
    assert!(mgr.is_empty());
    assert_eq!(mgr.rewind_entity(42, 100.0), None);

    mgr.record(42, 1, 16.0, TransformSnapshot::new(1.0, 2.0, 3.0, 1.0));
    assert_eq!(mgr.len(), 1);
    assert!(mgr.rewind_entity(42, 16.0).is_some());

    mgr.remove_entity(42);
    assert_eq!(mgr.rewind_entity(42, 16.0), None);
    assert!(mgr.is_empty());
}

#[test]
fn rewind_clear_drops_everything() {
    let mut mgr = RewindManager::new(500, 60);
    for id in 0..10u64 {
        mgr.record(id, 1, 16.0, TransformSnapshot::new(0.0, 0.0, 0.0, 1.0));
    }
    assert_eq!(mgr.len(), 10);
    mgr.clear();
    assert!(mgr.is_empty());
    assert_eq!(mgr.rewind_entity(3, 16.0), None);
}

#[test]
fn rewind_rejects_non_monotonic_ticks() {
    let mut mgr = RewindManager::new(500, 60);
    mgr.record(1, 10, 16.0, TransformSnapshot::new(10.0, 0.0, 0.0, 1.0));
    mgr.record(1, 20, 32.0, TransformSnapshot::new(20.0, 0.0, 0.0, 1.0));

    mgr.record(1, 5, 8.0, TransformSnapshot::new(999.0, 0.0, 0.0, 1.0));
    mgr.record(1, 20, 32.0, TransformSnapshot::new(999.0, 0.0, 0.0, 1.0));
    let snap = mgr.rewind_entity(1, 32.0).unwrap();
    assert_eq!(snap.x, 20.0);
}

#[test]
fn rewind_all_matches_rewind_all_into() {
    let mut mgr = RewindManager::new(500, 60);
    for id in 0..4u64 {
        mgr.record(
            id,
            1,
            16.0,
            TransformSnapshot::new(id as f32, 0.0, 0.0, 1.0),
        );
        mgr.record(
            id,
            2,
            32.0,
            TransformSnapshot::new(id as f32 * 2.0, 0.0, 0.0, 1.0),
        );
    }
    let allocating = mgr.rewind_all(24.0);
    let mut reused = Vec::new();
    mgr.rewind_all_into(24.0, &mut reused);
    assert_eq!(allocating.len(), 4);
    assert_eq!(allocating, reused);
    for (id, snap) in &reused {
        let expected = *id as f32 * 1.5;
        assert!((snap.x - expected).abs() < 1e-4, "id {id} x {}", snap.x);
    }
}

#[test]
fn point_hit_boundary_is_inclusive() {
    let snap = TransformSnapshot::new(0.0, 0.0, 0.0, 10.0);

    assert!(snap.is_point_hit(10.0, 0.0, 0.0));
    assert!(!snap.is_point_hit(10.001, 0.0, 0.0));
    assert!(!snap.is_point_hit(6.0, 6.0, 6.0));
    assert!(snap.is_point_hit(5.0, 5.0, 5.0));
}
