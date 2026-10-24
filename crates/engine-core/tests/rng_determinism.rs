use engine_core::RoomRng;

#[test]
fn test_rng_determinism_same_seed_produces_identical_sequence() {
    let mut rng_a = RoomRng::from_seed_str("room-battle-royale-alpha-123");
    let mut rng_b = RoomRng::from_seed_str("room-battle-royale-alpha-123");

    let rolls_a: Vec<f32> = (0..1000).map(|_| rng_a.next_f32()).collect();
    let rolls_b: Vec<f32> = (0..1000).map(|_| rng_b.next_f32()).collect();

    assert_eq!(
        rolls_a, rolls_b,
        "Identical seeds must produce 100% identical 1,000 float sequences"
    );

    let ranges_a: Vec<i32> = (0..1000).map(|_| rng_a.next_range(1, 100)).collect();
    let ranges_b: Vec<i32> = (0..1000).map(|_| rng_b.next_range(1, 100)).collect();

    assert_eq!(
        ranges_a, ranges_b,
        "Identical seeds must produce 100% identical 1,000 range sequences"
    );
}

#[test]
fn test_rng_determinism_different_seed_produces_different_sequence() {
    let mut rng_a = RoomRng::from_seed_str("room-alpha");
    let mut rng_b = RoomRng::from_seed_str("room-beta");

    let rolls_a: Vec<f32> = (0..100).map(|_| rng_a.next_f32()).collect();
    let rolls_b: Vec<f32> = (0..100).map(|_| rng_b.next_f32()).collect();

    assert_ne!(
        rolls_a, rolls_b,
        "Different seeds must not produce identical sequences"
    );
}
