use crate::{
    feed, frame,
    vectors::{DELTA_FRAG_0, DELTA_FRAG_1},
};
use engine_webrtc_protocol::channel::{FragmentAssembler, is_newer, is_superseded};

/// The half-range rule, in both directions, and the fact that "superseded" is
/// its exact complement — server filter and client filter can never disagree.
#[test]
fn sequence_wrap_is_one_rule_both_ends_share() {
    assert!(is_newer(0, 1), "one past is newer");
    assert!(!is_newer(1, 1), "equal is not newer: a delta applies once");
    assert!(!is_newer(1, 0), "one behind is older");
    assert!(is_newer(u16::MAX, 0), "the forward wrap is newer");
    assert!(
        !is_newer(0, u16::MAX),
        "and reading it backwards is the classic bug"
    );
    assert!(
        is_newer(65530, 5),
        "a 511-tick gap across the wrap is still newer"
    );
    assert!(
        !is_newer(5, 65530),
        "while the same gap in reverse is stale"
    );
    assert!(!is_newer(0, 0x8000), "the half-range boundary is not newer");

    for (last, seq) in [(0u16, 1u16), (1, 0), (u16::MAX, 0), (0, u16::MAX), (7, 7)] {
        assert_eq!(is_superseded(last, seq), !is_newer(last, seq));
        assert!(is_superseded(last, last), "a duplicate is superseded");
    }
}

#[test]
fn fragments_reassemble_in_any_order() {
    let mut assembler = FragmentAssembler::default();
    assert_eq!(feed(&mut assembler, &DELTA_FRAG_1, 0), None);

    let body = feed(&mut assembler, &DELTA_FRAG_0, 10);
    assert_eq!(
        body,
        Some(vec![0xde, 0xad, 0xbe, 0xef, 0x00, 0x11, 0x22, 0x33])
    );
    assert_eq!(assembler.assembled(), 1);
    assert_eq!(assembler.newest_seq(), Some(65535));
    assert_eq!(assembler.inflight(), 0);
    assert_eq!(assembler.buffered_bytes(), 0);
}

#[test]
fn a_duplicate_fragment_does_not_double_count() {
    let mut assembler = FragmentAssembler::default();
    assert_eq!(feed(&mut assembler, &DELTA_FRAG_0, 0), None);
    assert_eq!(feed(&mut assembler, &DELTA_FRAG_0, 1), None);
    assert_eq!(assembler.duplicates(), 1);
    assert_eq!(
        assembler.buffered_bytes(),
        4,
        "the duplicate was not stored twice"
    );
    let body = feed(&mut assembler, &DELTA_FRAG_1, 2);
    assert_eq!(
        body,
        Some(vec![0xde, 0xad, 0xbe, 0xef, 0x00, 0x11, 0x22, 0x33])
    );
    assert_eq!(assembler.assembled(), 1);
}

#[test]
fn a_superseded_delta_is_dropped_unread() {
    let mut assembler = FragmentAssembler::default();
    let mut seq = 0u16;
    let mut emitted = 0;
    for _ in 0..1000 {
        seq = seq.wrapping_add(1);
        let wire = frame(seq, 0, 0, 1, &[0xa5]);
        if feed(&mut assembler, &wire, 0).is_some() {
            emitted += 1;
        }
        let late = frame(seq.wrapping_sub(1), 0, 0, 1, &[0xa5]);
        assert_eq!(feed(&mut assembler, &late, 0), None);
        assert!(assembler.is_superseded(seq.wrapping_sub(1)));
        assert!(!assembler.is_superseded(seq.wrapping_add(1)));
    }
    assert_eq!(emitted, 1000);
    assert_eq!(assembler.stale(), 1000);
}

#[test]
fn staleness_survives_the_sequence_wrap() {
    let mut assembler = FragmentAssembler::default();
    for wire in [&DELTA_FRAG_1, &DELTA_FRAG_0] {
        feed(&mut assembler, wire, 0);
    }
    assert_eq!(assembler.newest_seq(), Some(65535));

    let next = frame(0, 9, 0, 1, &[0xa5]);
    assert_eq!(feed(&mut assembler, &next, 0), Some(vec![0xa5]));

    let behind = frame(65534, 9, 0, 1, &[0xa5]);
    assert_eq!(feed(&mut assembler, &behind, 0), None);

    let ahead = frame(1, 9, 0, 2, &[0xa5]);
    let gap = frame(65530, 9, 0, 2, &[0x5a]);
    assert_eq!(feed(&mut assembler, &gap, 0), None);
    assert_eq!(assembler.stale(), 2);
    assert_eq!(feed(&mut assembler, &ahead, 0), None);
    let close = frame(1, 9, 1, 2, &[0xa6]);
    assert_eq!(feed(&mut assembler, &close, 0), Some(vec![0xa5, 0xa6]));
}

/// The tail of one delta arriving inside the next one is the point of the
/// unreliable channel: both must still close byte-exactly.
#[test]
fn interleaved_deltas_both_close() {
    let mut assembler = FragmentAssembler::default();
    let body_a: Vec<u8> = (0..30u32).map(|i| i as u8).collect();
    let body_b: Vec<u8> = (0..30u32).map(|i| 100 + i as u8).collect();
    let parts_a: Vec<Vec<u8>> = body_a.chunks(10).map(|c| c.to_vec()).collect();
    let parts_b: Vec<Vec<u8>> = body_b.chunks(10).map(|c| c.to_vec()).collect();

    let order = [(10u16, 0usize), (11, 0), (10, 1), (11, 1), (10, 2), (11, 2)];
    let mut delivered = Vec::new();
    for (arrival, (seq, idx)) in order.iter().enumerate() {
        let chunk = if *seq == 10 {
            &parts_a[*idx]
        } else {
            &parts_b[*idx]
        };
        let wire = frame(*seq, u32::from(*seq == 11), *idx as u8, 3, chunk);
        if let Some(delta) = assembler.insert_frame(&wire, arrival as u64) {
            delivered.push((delta.seq, delta.body));
        }
    }

    assert_eq!(delivered, vec![(10, body_a.clone()), (11, body_b.clone())]);
    assert_eq!(assembler.assembled(), 2);
    assert_eq!(assembler.inflight(), 0);
    assert_eq!(assembler.newest_seq(), Some(11));
}
