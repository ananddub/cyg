use engine_core::{PlayerInput, PlayerInputBuffer};

use super::harness::{ITER, MEASURE_LOCK, WARMUP, WINDOWS, min_allocs_per_iter};

#[test]
fn input_push_consume_is_zero_alloc_steady_state() {
    let _guard = MEASURE_LOCK.lock().unwrap();
    let mut buf = PlayerInputBuffer::new(64);

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
    assert_eq!(
        deallocs, 0.0,
        "input path freed something it never allocated"
    );
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
