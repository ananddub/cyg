//! Phase-2 host-service tests: the `sys_*` import surface backed by
//! `RoomHostData` — deterministic RNG, kiddo spatial AOI, tick clock.

use engine_server::rooms::RoomHostData;
use engine_spatial::SpatialIndex2D;

#[test]
fn seeded_rng_is_deterministic_per_room_name() {
    let mut a = RoomHostData::new(engine_physics::PhysicsWorld3D::default());
    let mut b = RoomHostData::new(engine_physics::PhysicsWorld3D::default());
    a.seed_rng("match-ranked-asia-49210");
    b.seed_rng("match-ranked-asia-49210");

    let seq_a: Vec<f64> = (0..16).map(|_| a.rng.next_f32() as f64).collect();
    let seq_b: Vec<f64> = (0..16).map(|_| b.rng.next_f32() as f64).collect();
    assert_eq!(seq_a, seq_b, "same room seed -> identical sequences");

    let mut c = RoomHostData::new(engine_physics::PhysicsWorld3D::default());
    c.seed_rng("match-ranked-asia-49211");
    let seq_c: Vec<f64> = (0..16).map(|_| c.rng.next_f32() as f64).collect();
    assert_ne!(seq_a, seq_c, "different room -> different sequence");
    for v in &seq_a {
        assert!(*v >= 0.0 && *v < 1.0, "rng values must be in [0,1)");
    }
}

#[test]
fn spatial_upsert_is_idempotent_per_entity() {
    let mut idx = SpatialIndex2D::new();
    // Same entity moves each tick via upsert: must never duplicate.
    for t in 0..10 {
        idx.upsert(1, [t as f64 * 10.0, 0.0]);
    }
    let hits = idx.query_radius([90.0, 0.0], 5.0);
    assert_eq!(hits.len(), 1, "moved entity must appear exactly once");
    assert_eq!(hits[0].entity_id, 1);

    // Radius filtering: entity 2 far away must not match entity 1's query.
    idx.upsert(2, [500.0, 500.0]);
    let near = idx.query_radius([90.0, 0.0], 5.0);
    assert_eq!(near.len(), 1, "out-of-radius entities are excluded");

    // Multiple neighbors inside the radius all come back.
    idx.upsert(3, [95.0, 0.0]);
    idx.upsert(4, [85.0, 0.0]);
    let all = idx.query_radius([90.0, 0.0], 10.0);
    assert_eq!(all.len(), 3, "entities 1, 3, 4 within radius 10 of (90,0)");
}

#[test]
fn clock_starts_at_zero_and_is_host_owned() {
    let d = RoomHostData::new(engine_physics::PhysicsWorld3D::default());
    assert_eq!(d.clock_ticks, 0, "guest sees tick 0 before the first tick");
    // GameRoom::tick bumps it; that path is covered by the e2e suites —
    // the contract here is only that the default is deterministic.
}
