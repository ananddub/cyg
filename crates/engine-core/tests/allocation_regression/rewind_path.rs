use engine_core::{PlayerInput, PlayerInputBuffer, RewindManager, TransformSnapshot};

use super::harness::{allocs_in, min_allocs_per_iter, MEASURE_LOCK, WARMUP, WINDOWS};

#[test]
fn rewind_record_and_rewind_are_zero_alloc_steady_state() {
    let _guard = MEASURE_LOCK.lock().unwrap();
    let mut mgr = RewindManager::new(500, 60);

    for tick in 0..WARMUP as u64 {
        mgr.record(
            7,
            tick,
            tick as f64 * 16.666,
            TransformSnapshot::new(tick as f32, 0.0, 0.0, 1.0),
        );
    }
    let mut tick = WARMUP as u64;
    let (allocs, deallocs) = min_allocs_per_iter(WINDOWS, 512, || {
        mgr.record(
            7,
            tick,
            tick as f64 * 16.666,
            TransformSnapshot::new(tick as f32, 0.0, 0.0, 1.0),
        );
        let _ = mgr.rewind_entity(7, tick as f64 * 16.666 - 8.0);
        tick += 1;
    });
    assert_eq!(
        allocs, 0.0,
        "rewind record+query allocates {allocs}/iter — the ring buffer went to the heap"
    );
    assert_eq!(deallocs, 0.0, "unexpected deallocations on the rewind path");
}

#[test]
fn measurement_window_is_leak_free() {
    let _guard = MEASURE_LOCK.lock().unwrap();

    let mut buf = PlayerInputBuffer::new(64);
    let mut mgr = RewindManager::new(500, 60);

    mgr.record(7, 0, 0.0, TransformSnapshot::new(0.0, 0.0, 0.0, 1.0));
    buf.push(PlayerInput::new(0, 0.0, [0u8; 24]));
    buf.consume();
    let mut seq = 50_000u64;
    let (allocs, deallocs) = allocs_in(2_000, || {
        buf.push(PlayerInput::new(seq, 0.0, [0u8; 24]));
        buf.consume();
        mgr.record(
            7,
            seq,
            seq as f64 * 16.666,
            TransformSnapshot::new(seq as f32, 0.0, 0.0, 1.0),
        );
        let _ = mgr.rewind_entity(7, seq as f64 * 16.666 - 8.0);
        seq += 1;
    });

    let net = allocs as i64 - deallocs as i64;
    assert!(
        net.abs() <= 64,
        "net {net} over 2,000 hot-path iterations — leak or runaway allocation"
    );
}
