use engine_core::*;

#[test]
fn rng_from_u64_is_deterministic_and_independent_of_seed_str() {
    let mut a = RoomRng::from_u64(0xDEADBEEF);
    let mut b = RoomRng::from_u64(0xDEADBEEF);
    let mut c = RoomRng::from_seed_str("0xDEADBEEF");
    let seq_a: Vec<u32> = (0..64).map(|_| a.next_u32()).collect();
    let seq_b: Vec<u32> = (0..64).map(|_| b.next_u32()).collect();
    let seq_c: Vec<u32> = (0..64).map(|_| c.next_u32()).collect();
    assert_eq!(seq_a, seq_b, "same numeric seed => identical stream");
    assert_ne!(seq_a, seq_c, "string seeding is a different construction");
}

#[test]
fn rng_degenerate_range_returns_lo_without_panic() {
    let mut rng = RoomRng::from_u64(7);
    assert_eq!(rng.next_range(5, 5), 5);
    assert_eq!(rng.next_range(9, 3), 9);

    for _ in 0..1000 {
        let v = rng.next_range(-10, 10);
        assert!((-10..10).contains(&v));
    }
}

#[test]
fn rng_bool_clamps_probability_without_panic() {
    let mut rng = RoomRng::from_u64(3);
    for _ in 0..100 {
        assert!(!rng.next_bool(0.0));
        assert!(rng.next_bool(1.0));
    }

    assert!(!rng.next_bool(-1.0));
    assert!(rng.next_bool(2.0));
    assert!(!rng.next_bool(f64::NAN));
}

#[test]
fn rng_clone_continues_identically() {
    let mut rng = RoomRng::from_seed_str("match-42");
    for _ in 0..10 {
        rng.next_f32();
    }
    let mut copy = rng.clone();
    let tail_orig: Vec<f32> = (0..32).map(|_| rng.next_f32()).collect();
    let tail_copy: Vec<f32> = (0..32).map(|_| copy.next_f32()).collect();
    assert_eq!(tail_orig, tail_copy);
}
