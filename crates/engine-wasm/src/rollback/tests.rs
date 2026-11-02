use super::*;

#[test]
fn test_checksum_computation() {
    let b1 = b"Player1_Position_X_10_Y_20";
    let b2 = b"Player1_Position_X_10_Y_20";
    let b3 = b"Player1_Position_X_10_Y_21";

    let h1 = compute_checksum_64(b1);
    let h2 = compute_checksum_64(b2);
    let h3 = compute_checksum_64(b3);

    assert_eq!(h1, h2, "Identical bytes must produce identical checksums");
    assert_ne!(h1, h3, "Different bytes must produce distinct checksums");
}

#[test]
fn test_rollback_ring_buffer_range_and_slotting() {
    let mut buf = RollbackRingBuffer::new(4);
    assert_eq!(buf.available_range(), None);

    buf.frames[0] = Some(SnapshotFrame {
        tick: 4,
        memory: vec![1, 2, 3],
        checksum: 100,
    });
    buf.frames[1] = Some(SnapshotFrame {
        tick: 5,
        memory: vec![4, 5, 6],
        checksum: 200,
    });

    assert_eq!(buf.get_checksum(4), Some(100));
    assert_eq!(buf.get_checksum(5), Some(200));
    assert_eq!(buf.get_checksum(0), None);

    assert!(buf.verify_sync(4, 100).is_ok());
    let desync = buf.verify_sync(4, 999).unwrap_err();
    assert_eq!(desync.tick, 4);
    assert_eq!(desync.expected_checksum, 100);
    assert_eq!(desync.client_checksum, 999);
}

#[test]
fn test_blob_checkpoints_answer_checksum_reports() {
    let mut buf = RollbackRingBuffer::new(4);
    buf.save_blob(7, b"state_at_7".to_vec());

    assert_eq!(
        buf.get_checksum(7),
        Some(compute_checksum_64(b"state_at_7"))
    );
    assert!(
        buf.verify_sync(7, compute_checksum_64(b"state_at_7"))
            .is_ok()
    );

    let desync = buf
        .verify_sync(7, compute_checksum_64(b"state_at_8"))
        .unwrap_err();
    assert_eq!(desync.tick, 7);
    assert_eq!(desync.expected_checksum, compute_checksum_64(b"state_at_7"));
    assert_eq!(desync.client_checksum, compute_checksum_64(b"state_at_8"));
}

#[test]
fn test_a_tick_with_no_checkpoint_is_not_called_a_desync() {
    let mut buf = RollbackRingBuffer::new(4);
    buf.save_blob(7, b"state_at_7".to_vec());
    assert!(buf.verify_sync(1_000, 0).is_ok());
}
