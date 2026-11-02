use engine_core::{FixedClock, PlayerInput, PlayerInputBuffer};

#[test]
fn test_player_input_buffer_sequencing_and_reconciliation_ack() {
    let mut buffer = PlayerInputBuffer::new(64);

    assert!(buffer.push(PlayerInput::new(1, 10.0, vec![1, 0])));
    assert!(buffer.push(PlayerInput::new(2, 26.6, vec![1, 0])));
    assert!(buffer.push(PlayerInput::new(3, 43.3, vec![0, 1])));
    assert!(!buffer.push(PlayerInput::new(2, 26.6, vec![1, 0])));
    assert!(buffer.push(PlayerInput::new(5, 76.6, vec![0, 1])));
    assert!(buffer.push(PlayerInput::new(4, 60.0, vec![1, 1])));
    assert_eq!(buffer.pending_count(), 5);

    let input_1 = buffer.consume().expect("Input 1");
    assert_eq!(input_1.seq, 1);
    assert_eq!(buffer.consumed_count(), 1);

    let input_2 = buffer.consume().expect("Input 2");
    assert_eq!(input_2.seq, 2);
    assert_eq!(buffer.consumed_count(), 2);

    assert!(!buffer.push(PlayerInput::new(1, 10.0, vec![1, 0])));

    let remaining = buffer.consume_batch(10);
    assert_eq!(remaining.len(), 3);
    assert_eq!(remaining[0].seq, 3);
    assert_eq!(remaining[1].seq, 4);
    assert_eq!(remaining[2].seq, 5);

    assert_eq!(buffer.consumed_count(), 5);
    assert_eq!(buffer.last_consumed_seq(), 5);
    assert_eq!(buffer.pending_count(), 0);
}

#[test]
fn test_fixed_clock_ticks() {
    let mut clock = FixedClock::new(60);
    assert_eq!(clock.tick_rate, 60);

    let tick_1 = clock.advance();
    assert_eq!(tick_1.tick, 1);
    assert!((tick_1.dt - 0.016666668).abs() < 0.0001);

    let tick_2 = clock.advance();
    assert_eq!(tick_2.tick, 2);
}
