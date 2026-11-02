use engine_core::*;

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

    assert!(buf.push(input(17)));
    assert_eq!(buf.pending_count(), 16);
    assert_eq!(buf.peek().unwrap().seq, 2);
}

#[test]
fn input_backlog_never_evicts_for_stale_redelivery() {
    let mut buf = PlayerInputBuffer::new(16);
    for seq in 1..=16 {
        assert!(buf.push(input(seq)));
    }

    assert!(!buf.push(input(0)));
    assert_eq!(buf.pending_count(), 16);
    assert_eq!(buf.peek().unwrap().seq, 1);

    for seq in 1..=16u64 {
        assert_eq!(buf.consume().unwrap().seq, seq);
    }
    assert_eq!(buf.consumed_count(), 16);
}

#[test]
fn input_backlog_boundary_arrival_evicts_oldest() {
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
    let mut batch = Vec::with_capacity(32);
    buf.consume_batch_into(16, &mut batch);
    assert_eq!(batch.len(), 16);
    assert_eq!(batch[0].seq, 1);
    let cap_before = batch.capacity();
    buf.consume_batch_into(32, &mut batch);
    assert_eq!(batch.len(), 32);

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

    assert!(buf.push(input(9)));
    assert_eq!(buf.consume().unwrap().seq, 9);
    assert_eq!(buf.consumed_count(), 1);
}
