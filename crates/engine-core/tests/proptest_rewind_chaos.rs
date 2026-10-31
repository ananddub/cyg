//! Property-based chaos for the rewind ring (proptest + shrinking).
//!
//! Random monotonic recording cadences (uniform and irregular gaps) and random
//! query times. The snapshot `x` doubles as the sample's time coordinate, so
//! "rewind(t)" must reproduce `t` within one sample gap — a lag-comp quality
//! guarantee, not just "doesn't panic". Failures shrink to a minimal cadence.

use engine_core::{Interpolation, RewindManager, TransformSnapshot};
use proptest::prelude::*;

/// Random strictly-increasing tick cadence with jittered gaps.
fn cadence_strategy() -> impl Strategy<Value = Vec<u64>> {
    prop::collection::vec(1u64..40, 2..80).prop_map(|gaps| {
        let mut ticks = Vec::with_capacity(gaps.len() + 1);
        let mut t = 0u64;
        for g in gaps {
            ticks.push(t);
            t += g;
        }
        ticks.push(t);
        ticks
    })
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn rewind_reproduces_time_within_one_gap(ticks in cadence_strategy(), probe in 0u64..2000) {
        let mut mgr = RewindManager::new(60_000, 60); // window never evicts here
        for (i, &tick) in ticks.iter().enumerate() {
            let t_ms = tick as f64 * 10.0;
            // x mirrors time: a perfect rewind returns the queried time itself.
            mgr.record(1, tick, t_ms, TransformSnapshot::new(t_ms as f32, 0.0, 0.0, 1.0));
            let _ = i;
        }

        let newest_ms = *ticks.last().unwrap() as f64 * 10.0;
        let target = (probe as f64 * 7.0).min(newest_ms); // in-range query time

        let snap = mgr
            .rewind_entity(1, target)
            .expect("in-range query must hit a recorded history");

        // Monotone x == time, so the interpolated x must match the target
        // within one sampling gap (16.67ms at 60Hz, doubled for f32 error).
        let gap_ms = 16.67f64 * 2.0 + 1.0;
        prop_assert!(
            (snap.x as f64 - target).abs() <= gap_ms,
            "rewind to {} returned x={}, off by more than one gap",
            target,
            snap.x
        );
    }

    #[test]
    fn rewind_clamps_outside_the_recorded_range(ticks in cadence_strategy(), past in 1u64..500) {
        let mut mgr = RewindManager::new(60_000, 60);
        for &tick in &ticks {
            let t_ms = tick as f64 * 10.0;
            mgr.record(1, tick, t_ms, TransformSnapshot::new(t_ms as f32, 0.0, 0.0, 1.0));
        }
        let oldest_ms = ticks[0] as f64 * 10.0;
        let newest_ms = *ticks.last().unwrap() as f64 * 10.0;

        // Before the oldest: clamps to the oldest sample (f32 tolerance — the
        // x column stores time as f32, which cannot represent ms exactly).
        let before = mgr.rewind_entity(1, oldest_ms - past as f64 * 100.0).unwrap();
        prop_assert!((before.x as f64 - oldest_ms).abs() < 0.1);

        // After the newest: clamps to the newest sample.
        let after = mgr.rewind_entity(1, newest_ms + past as f64 * 100.0).unwrap();
        prop_assert!((after.x as f64 - newest_ms).abs() < 0.1);
    }

    #[test]
    fn step_interpolation_holds_pre_time_sample(ticks in cadence_strategy(), probe in 1u64..2000) {
        let mut mgr = RewindManager::new_with_interpolation(60_000, 60, Interpolation::Step);
        for &tick in &ticks {
            let t_ms = tick as f64 * 10.0;
            mgr.record(1, tick, t_ms, TransformSnapshot::new(t_ms as f32, 0.0, 0.0, 1.0));
        }
        // Between two samples, Step mode holds the earlier sample's value.
        let newest_ms = *ticks.last().unwrap() as f64 * 10.0;
        let target = (probe as f64 * 3.0) % newest_ms.max(1.0);
        let snapped = mgr.rewind_entity(1, target).unwrap();
        prop_assert!(snapped.x as f64 <= target + 1e-3,
            "Step must hold the pre-time sample, got {} for target {}", snapped.x, target);
    }
}
