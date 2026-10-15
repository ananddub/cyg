use super::*;
use crate::telemetry::stats::RING;

#[test]
fn percentiles_on_synthetic_samples() {
    let mut r = Recorder::new();
    for i in 0..100u64 {
        r.note_input_latency(0, (i + 1) * 1_000); // 1..=100 µs latencies
        r.note_tick(Duration::from_micros(i + 1));
    }
    let s = r.snapshot();
    assert_eq!(s.kernel_to_sim_p50_us, 50);
    assert_eq!(s.kernel_to_sim_p99_us, 99);
    assert_eq!(s.tick_budget_p50_us, 50);
    assert_eq!(s.tick_budget_p99_us, 99);
}

#[test]
fn percentile_nearest_rank_small_window() {
    let mut r = Recorder::new();
    r.note_tick(Duration::from_micros(10));
    let s = r.snapshot();
    assert_eq!(s.tick_budget_p50_us, 10);
    assert_eq!(s.tick_budget_p99_us, 10);
    let empty = Recorder::new().snapshot();
    assert_eq!((empty.tick_budget_p50_us, empty.kernel_to_sim_p99_us), (0, 0));
}

#[test]
fn ring_wraps_and_keeps_window() {
    let mut r = Recorder::new();
    // Overfill the ring: newest samples are all 1000 ns.
    for _ in 0..(RING as u64 * 2) {
        r.note_input_latency(0, 1_000);
    }
    let s = r.snapshot();
    assert_eq!(s.kernel_to_sim_p50_us, 1); // ns → µs
    assert_eq!(s.kernel_to_sim_p99_us, 1);
}

#[test]
fn stale_arrival_saturates() {
    let mut r = Recorder::new();
    r.note_input_latency(10_000, 3); // clock went backwards: no panic, no underflow
    assert_eq!(r.snapshot().kernel_to_sim_p50_us, 0);
}

#[test]
fn overrun_counted_against_tick_period() {
    let mut r = Recorder::new(); // 128 Hz → budget 7.8125 ms
    r.note_tick(Duration::from_millis(20));
    r.note_tick(Duration::from_millis(1));
    assert_eq!(r.snapshot().tick_overruns, 1);
}
