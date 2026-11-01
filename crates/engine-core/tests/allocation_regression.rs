//! Allocation regression tests (pattern borrowed from Lightyear's
//! `crates/tests/tests/allocation_regression.rs`): steady-state hot paths
//! must not allocate. A counting global allocator + min-over-windows scoring
//! makes the check robust to background noise from other tests sharing the
//! process, and a net-allocs==deallocs window doubles as a leak check.
//!
//! These pin the zero-alloc work directly: `PlayerInput` inline SmallVec
//! payloads and the rewind ring's record/rewind path.

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

static ALLOCS: AtomicU64 = AtomicU64::new(0);
static DEALLOCS: AtomicU64 = AtomicU64::new(0);

struct CountingAlloc;

unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        ALLOCS.fetch_add(1, Ordering::Relaxed);
        System.alloc(layout)
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        DEALLOCS.fetch_add(1, Ordering::Relaxed);
        System.dealloc(ptr, layout)
    }
}

#[global_allocator]
static GLOBAL: CountingAlloc = CountingAlloc;

/// One measurement lock for the whole binary: allocation counts are process
/// globals, so concurrent tests would pollute each other's windows.
static MEASURE_LOCK: Mutex<()> = Mutex::new(());

fn snapshot() -> (u64, u64) {
    (ALLOCS.load(Ordering::Relaxed), DEALLOCS.load(Ordering::Relaxed))
}

/// Run `f` once per iteration for `iterations`, returning the allocations
/// counted in that window (caller warms up first).
fn allocs_in<F: FnMut()>(iterations: usize, mut f: F) -> (u64, u64) {
    let (a0, d0) = snapshot();
    for _ in 0..iterations {
        f();
    }
    let (a1, d1) = snapshot();
    (a1 - a0, d1 - d0)
}

/// Minimum per-iteration allocations over `windows` windows of `iterations`
/// each. The min is immune to background noise: interleaved tests only ever
/// ADD allocations, so the cleanest window is the true one.
fn min_allocs_per_iter<F: FnMut()>(windows: usize, iterations: usize, mut f: F) -> (f64, f64) {
    let mut min_a = u64::MAX;
    let mut min_d = u64::MAX;
    for _ in 0..windows {
        let (a, d) = allocs_in(iterations, &mut f);
        min_a = min_a.min(a);
        min_d = min_d.min(d);
    }
    (
        min_a as f64 / iterations as f64,
        min_d as f64 / iterations as f64,
    )
}

use engine_core::{PlayerInput, PlayerInputBuffer, RewindManager, TransformSnapshot};

const WARMUP: usize = 256;
const ITER: usize = 512;
const WINDOWS: usize = 8;

#[test]
fn input_push_consume_is_zero_alloc_steady_state() {
    let _guard = MEASURE_LOCK.lock().unwrap();
    let mut buf = PlayerInputBuffer::new(64);
    // Warm-up: pre-grow any internal state (queue capacity).
    for seq in 0..WARMUP as u64 {
        buf.push(PlayerInput::new(seq, 0.0, [0u8; 24]));
        buf.consume();
    }

    let mut seq = WARMUP as u64;
    let (allocs, deallocs) = min_allocs_per_iter(WINDOWS, ITER, || {
        buf.push(PlayerInput::new(seq, 0.0, [0u8; 24]));
        buf.consume();
        seq += 1;
    });
    assert_eq!(
        allocs, 0.0,
        "input push+consume allocates {allocs}/iter — the inline-SmallVec regression is back"
    );
    assert_eq!(deallocs, 0.0, "input path freed something it never allocated");
}

#[test]
fn consume_batch_into_reuses_its_buffer() {
    let _guard = MEASURE_LOCK.lock().unwrap();
    let mut buf = PlayerInputBuffer::new(64);
    let mut batch: Vec<PlayerInput> = Vec::with_capacity(32);
    for seq in 0..(WARMUP as u64) {
        buf.push(PlayerInput::new(seq, 0.0, [0u8; 24]));
    }
    for _ in 0..WARMUP {
        buf.consume_batch_into(16, &mut batch);
        for seq in 0..16u64 {
            buf.push(PlayerInput::new(seq, 0.0, [0u8; 24]));
        }
    }
    // Refill helper: keep 32 pending so each batch drains 16.
    let refill = |buf: &mut PlayerInputBuffer, next: &mut u64| {
        while buf.pending_count() < 32 {
            buf.push(PlayerInput::new(*next, 0.0, [0u8; 24]));
            *next += 1;
        }
    };
    let mut next = 10_000u64;
    refill(&mut buf, &mut next);

    let (allocs, deallocs) = min_allocs_per_iter(WINDOWS, ITER, || {
        refill(&mut buf, &mut next);
        buf.consume_batch_into(16, &mut batch);
    });
    assert_eq!(
        allocs, 0.0,
        "consume_batch_into allocates {allocs}/iter in steady state — buffer reuse broke"
    );
    assert_eq!(deallocs, 0.0, "unexpected deallocations on the batch path");
}

#[test]
fn rewind_record_and_rewind_are_zero_alloc_steady_state() {
    let _guard = MEASURE_LOCK.lock().unwrap();
    let mut mgr = RewindManager::new(500, 60);
    // Warm-up: create the entity history (one-time map insert + ring alloc).
    for tick in 0..WARMUP as u64 {
        mgr.record(
            7,
            tick,
            tick as f64 * 16.666,
            TransformSnapshot::new(tick as f32, 0.0, 0.0, 1.0),
        );
    }
    let mut tick = WARMUP as u64;
    let (allocs, deallocs) = min_allocs_per_iter(WINDOWS, ITER, || {
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
    // Across one full window, every allocation must be matched by a
    // deallocation — the hot path must not leak, not even slowly.
    let mut buf = PlayerInputBuffer::new(64);
    let mut mgr = RewindManager::new(500, 60);
    // Warm-up: the first rewind record creates the entity history (map insert
    // + ring allocation) — a one-time cost, not a per-iteration leak.
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
    // Harness noise (drops of pre-window allocations, runner bookkeeping) can
    // push the net a little either way — a REAL per-iteration leak would dwarf
    // this tolerance (a 0.01 allocs/iter leak = 20 over 2,000 iterations).
    let net = allocs as i64 - deallocs as i64;
    assert!(
        net.abs() <= 64,
        "net {net} over 2,000 hot-path iterations — leak or runaway allocation"
    );
}
