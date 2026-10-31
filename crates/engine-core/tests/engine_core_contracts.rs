//! Contract tests for `engine-core`, ported from the behavioral guarantees the
//! Colyseus test suite pins for its room/tick/rewind/input layers, plus every
//! previously-untested edge of our own implementation:
//!
//! - Rewind ring eviction, clamping, unknown entities, monotonic recording
//! - Input backlog policy: the buffer always retains the *newest* inputs
//! - Deterministic RNG: seed independence, degenerate args, clone continuation
//! - TickRunner cleanup edges, Health/component semantics, FixedClock guards

use engine_core::*;
use bevy_ecs::prelude::*;

// ---------------------------------------------------------------------------
// Rewind: ring buffer eviction (Colyseus: "history is a bounded ring")
// ---------------------------------------------------------------------------

#[test]
fn rewind_capacity_matches_window_formula() {
    // The manager tracks one history per entity; each is bounded by the
    // derived window capacity (pinned precisely by the EntityHistory test
    // below — the manager just delegates to the same ring).
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
    // Long-evicted sample no longer reachable: the rewind clamps to the
    // oldest retained sample, not the oldest ever recorded.
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
    // Retained samples are ticks 3,4,5 (times 300..500).
    let (prev, next, _) = hist.find_samples(350.0).unwrap();
    assert_eq!(prev, 3.0);
    assert_eq!(next, 4.0);
    // Clamped to the NEW oldest (3.0), never to an evicted sample.
    let (prev, _, _) = hist.find_samples(0.0).unwrap();
    assert_eq!(prev, 3.0);
    // Clamped to newest.
    let (prev, next, _) = hist.find_samples(99_999.0).unwrap();
    assert_eq!(prev, 5.0);
    assert_eq!(next, 5.0);
}

#[test]
fn rewind_unknown_entity_and_remove_are_none() {
    let mut mgr = RewindManager::new(500, 60);
    assert!(mgr.is_empty());
    assert_eq!(mgr.rewind_entity(42, 100.0), None);

    mgr.record(
        42,
        1,
        16.0,
        TransformSnapshot::new(1.0, 2.0, 3.0, 1.0),
    );
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
    // A stale/out-of-order sample (clock jump backwards, duplicate delivery)
    // must be discarded, not written into the ring.
    mgr.record(1, 5, 8.0, TransformSnapshot::new(999.0, 0.0, 0.0, 1.0));
    mgr.record(1, 20, 32.0, TransformSnapshot::new(999.0, 0.0, 0.0, 1.0));
    let snap = mgr.rewind_entity(1, 32.0).unwrap();
    assert_eq!(snap.x, 20.0);
}

#[test]
fn rewind_all_matches_rewind_all_into() {
    let mut mgr = RewindManager::new(500, 60);
    for id in 0..4u64 {
        mgr.record(id, 1, 16.0, TransformSnapshot::new(id as f32, 0.0, 0.0, 1.0));
        mgr.record(id, 2, 32.0, TransformSnapshot::new(id as f32 * 2.0, 0.0, 0.0, 1.0));
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
    // dist == hit_radius exactly => hit (<= contract).
    assert!(snap.is_point_hit(10.0, 0.0, 0.0));
    // One ulp outside => miss.
    assert!(!snap.is_point_hit(10.001, 0.0, 0.0));
    // dist = sqrt(108) ~ 10.39 > 10 => miss.
    assert!(!snap.is_point_hit(6.0, 6.0, 6.0));
    // dist ~ 8.66 < 10 => hit.
    assert!(snap.is_point_hit(5.0, 5.0, 5.0));
}

// ---------------------------------------------------------------------------
// Input: backlog retention policy (Colyseus: bounded queues never lose the
// newest data — "the last N messages win")
// ---------------------------------------------------------------------------

fn input(seq: u64) -> PlayerInput {
    PlayerInput::new(seq, seq as f64 * 16.0, vec![seq as u8])
}

#[test]
fn input_backlog_evicts_oldest_when_newer_arrives() {
    let mut buf = PlayerInputBuffer::new(16);
    for seq in 1..=16 {
        assert!(buf.push(input(seq)));
    }
    assert_eq!(buf.pending_count(), 16);
    // 17th input: the oldest pending (seq 1) is evicted, queue holds 2..=17.
    assert!(buf.push(input(17)));
    assert_eq!(buf.pending_count(), 16);
    assert_eq!(buf.peek().unwrap().seq, 2);
}

#[test]
fn input_backlog_never_evicts_for_stale_redelivery() {
    // Regression: a late-arriving input (reconnect redelivery, reorder spike)
    // must be dropped itself, never evict a newer pending input — the client's
    // reconciliation ack would otherwise skip a real input (desync).
    let mut buf = PlayerInputBuffer::new(16);
    for seq in 1..=16 {
        assert!(buf.push(input(seq)));
    }
    // Stale-but-unconsumed arrival (below every queued seq).
    assert!(!buf.push(input(0)));
    assert_eq!(buf.pending_count(), 16);
    assert_eq!(buf.peek().unwrap().seq, 1);
    // The queue is intact: ordered drain still sees 1..=16.
    for seq in 1..=16u64 {
        assert_eq!(buf.consume().unwrap().seq, seq);
    }
    assert_eq!(buf.consumed_count(), 16);
}

#[test]
fn input_backlog_boundary_arrival_evicts_oldest() {
    // Arriving seq == front seq is rejected as a duplicate; arriving exactly
    // above front but queue full evicts front — retention still newest-wins.
    let mut buf = PlayerInputBuffer::new(16);
    for seq in 2..=17 {
        assert!(buf.push(input(seq)));
    }
    assert!(buf.push(input(18)));
    assert_eq!(buf.pending_count(), 16);
    assert_eq!(buf.peek().unwrap().seq, 3);
}

#[test]
fn input_consume_batch_into_reuses_buffer() {
    let mut buf = PlayerInputBuffer::new(64);
    for seq in 1..=64 {
        assert!(buf.push(input(seq)));
    }
    let mut batch = Vec::with_capacity(32); // pre-grown
    buf.consume_batch_into(16, &mut batch);
    assert_eq!(batch.len(), 16);
    assert_eq!(batch[0].seq, 1);
    let cap_before = batch.capacity();
    buf.consume_batch_into(32, &mut batch);
    assert_eq!(batch.len(), 32);
    // The buffer was reused: no reallocation up to the pre-grown capacity.
    assert!(batch.capacity() >= cap_before);
    assert_eq!(buf.consumed_count(), 48);
}

#[test]
fn input_consume_past_clear_resumes() {
    let mut buf = PlayerInputBuffer::new(16);
    for seq in 1..=8 {
        assert!(buf.push(input(seq)));
    }
    buf.clear();
    assert_eq!(buf.pending_count(), 0);
    // Ack state survived the clear: consumed seqs stay consumed, the counter
    // continues from where it was.
    assert!(buf.push(input(9)));
    assert_eq!(buf.consume().unwrap().seq, 9);
    assert_eq!(buf.consumed_count(), 1);
}

// ---------------------------------------------------------------------------
// RNG: determinism, seed independence, degenerate args (server APIs fed by
// room scripts must degrade, never panic)
// ---------------------------------------------------------------------------

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
    // Sane range still respects [lo, hi).
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
    // Out-of-range and NaN must degrade (never), not panic.
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

// ---------------------------------------------------------------------------
// TickRunner: cleanup edges
// ---------------------------------------------------------------------------

#[test]
fn tick_runner_empty_schedule_is_a_noop_tick() {
    let mut runner = TickRunner::new();
    for _ in 0..4 {
        runner.tick();
    }
    assert_eq!(runner.world.entities().len(), 0);
}

#[test]
fn tick_runner_despawns_every_marked_entity() {
    let mut runner = TickRunner::new();
    for _ in 0..50 {
        runner
            .world
            .spawn(Position2D::default())
            .insert(PendingDespawn);
    }
    runner
        .world
        .spawn(Position2D::default()); // unmarked survivor
    runner.tick();
    assert_eq!(runner.world.entities().len(), 1);
}

#[test]
fn tick_runner_survivor_when_marker_removed_before_tick() {
    let mut runner = TickRunner::new();
    let marked = runner
        .world
        .spawn(Position2D::default())
        .insert(PendingDespawn)
        .id();
    runner
        .world
        .entity_mut(marked)
        .remove::<PendingDespawn>();
    runner.tick();
    assert!(runner.world.get_entity(marked).is_ok());
}

#[test]
fn tick_runner_tolerates_entity_despawned_by_a_system() {
    // A system despawns the entity itself; the sweep must not double-despawn
    // or panic on an entity that vanished mid-tick.
    let mut runner = TickRunner::new();
    let victim = runner
        .world
        .spawn(Position2D::default())
        .insert(PendingDespawn)
        .id();
    runner.add_system(move |mut commands: Commands, q: Query<Entity, With<Position2D>>| {
        for e in &q {
            if e == victim {
                commands.entity(e).despawn();
            }
        }
    });
    runner.tick();
    assert_eq!(runner.world.entities().len(), 0);
}

// ---------------------------------------------------------------------------
// Components / Health
// ---------------------------------------------------------------------------

#[test]
fn health_overkill_clamps_at_zero_and_dies() {
    let mut hp = Health::new(100.0);
    assert!(hp.take_damage(250.0)); // returns is_dead
    assert!(hp.is_dead());
    assert_eq!(hp.current, 0.0);
    assert_eq!(hp.max, 100.0);
}

#[test]
fn health_death_boundary_is_exactly_zero() {
    let mut hp = Health::new(10.0);
    assert!(hp.take_damage(10.0)); // exactly 0.0 => dead
    assert!(hp.is_dead());
}

#[test]
fn health_negative_damage_heals_never_past_max() {
    let mut hp = Health::new(50.0);
    hp.take_damage(30.0); // 20
    assert_eq!(hp.current, 20.0);
    hp.take_damage(-100.0); // heal via negative damage
    assert_eq!(hp.current, 50.0); // clamped at max, never overfills
    assert!(!hp.is_dead());
}

#[test]
fn components_storage_roundtrip() {
    
    let mut world = World::new();
    let e = world
        .spawn((
            EntityId(7),
            Position3D::default(),
            Velocity::default(),
            PlayerControlled { client_id: 9001 },
            Health::new(100.0),
        ))
        .id();
    let (id, pos, vel, pc, hp) = world
        .entity(e)
        .get_components::<(
            &EntityId,
            &Position3D,
            &Velocity,
            &PlayerControlled,
            &Health,
        )>()
        .unwrap();
    assert_eq!(id.0, 7);
    assert_eq!(*pos, Position3D::default());
    assert_eq!(*vel, Velocity::default());
    assert_eq!(pc.client_id, 9001);
    assert_eq!(hp.current, 100.0);
}

// ---------------------------------------------------------------------------
// FixedClock
// ---------------------------------------------------------------------------

#[test]
#[should_panic(expected = "tick_rate must be > 0")]
fn fixed_clock_zero_rate_panics_with_clear_message() {
    let _ = FixedClock::new(0);
}

#[test]
fn fixed_clock_step_values_match_rate() {
    let mut clock = FixedClock::new(128);
    assert_eq!(clock.tick_rate, 128);
    let ctx = clock.advance();
    assert_eq!(ctx.tick, 1);
    assert!((ctx.dt - 1.0 / 128.0).abs() < 1e-6);
    assert!((ctx.dt_ms - 7.8125).abs() < 1e-9);
    let ctx = clock.advance();
    assert_eq!(ctx.tick, 2);
}
