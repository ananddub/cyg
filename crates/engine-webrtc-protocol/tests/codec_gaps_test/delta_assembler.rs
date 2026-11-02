use engine_webrtc_protocol::channel::{
    DeltaPacker, FragmentAssembler, MAX_DELTA_CHUNK, MAX_INFLIGHT_DELTAS,
};
use engine_webrtc_protocol::{AssembledDelta, DELTA_HEADER_LEN, write_delta_frame};

use crate::harness::first_fragment;

#[test]
fn empty_body_delta_roundtrips_end_to_end() {
    let mut packer = DeltaPacker::new(500, 42);
    let packing = packer.pack(&[]).expect("an empty body is one fragment");
    assert_eq!(packing.frag_total(), 1);
    assert_eq!(packing.wire_len(), DELTA_HEADER_LEN);

    let mut frame = Vec::new();
    packing.append_frames_to(&mut frame);
    assert_eq!(frame.len(), DELTA_HEADER_LEN);

    let mut assembler = FragmentAssembler::default();
    let assembled = assembler
        .insert_frame(&frame, 0)
        .expect("single-fragment delta delivers immediately");
    let AssembledDelta { seq, tick, body } = assembled;
    assert_eq!(seq, 500);
    assert_eq!(tick, 42);
    assert!(body.is_empty());
}

#[test]
fn byte_budget_evicts_oldest_partial_deltas() {
    let mut assembler = FragmentAssembler::default();

    const TOTAL: u8 = 130;
    for seq in 1000..1008u16 {
        let frame = first_fragment(seq, 1, TOTAL);
        assert!(
            assembler.insert_frame(&frame, 0).is_none(),
            "first fragment never completes"
        );
    }
    assert_eq!(assembler.inflight(), MAX_INFLIGHT_DELTAS);
    assert_eq!(assembler.evicted(), 0);

    let frame = first_fragment(1008, 1, TOTAL);
    assembler.insert_frame(&frame, 0);
    assert!(assembler.evicted() >= 1, "budget forced at least one eviction");
    assert_eq!(assembler.inflight(), MAX_INFLIGHT_DELTAS);
}

#[test]
fn budget_eviction_keeps_the_newest_delta_recoverable() {
    let mut assembler = FragmentAssembler::default();
    const TOTAL: u8 = 130;

    for seq in 2000..2008u16 {
        let frame = first_fragment(seq, 1, TOTAL);
        assembler.insert_frame(&frame, 0);
    }
    let frame = first_fragment(2008, 1, TOTAL);
    assembler.insert_frame(&frame, 0);

    for idx in 1..TOTAL {
        let start = idx as usize * MAX_DELTA_CHUNK;
        let chunk = vec![0xCDu8; MAX_DELTA_CHUNK];
        let _ = start;
        let mut frame = Vec::new();
        write_delta_frame(&mut frame, 2008, 1, idx, TOTAL, &chunk);
        let delivered = assembler.insert_frame(&frame, 0);
        if idx == TOTAL - 1 {
            let AssembledDelta { seq, tick, body } =
                delivered.expect("newest delta completes after evictions");
            assert_eq!(seq, 2008);
            assert_eq!(tick, 1);
            assert_eq!(body.len(), TOTAL as usize * MAX_DELTA_CHUNK);
        } else {
            assert!(delivered.is_none());
        }
    }
}

#[test]
fn a_fresh_assembler_trusts_the_stream_again() {
    let mut old = FragmentAssembler::default();
    let chunk = [1u8; 64];
    let mut frame = Vec::new();
    write_delta_frame(&mut frame, 65_000, 9, 0, 1, &chunk);
    assert!(old.insert_frame(&frame, 0).is_some());

    let mut fresh = FragmentAssembler::default();
    let mut frame = Vec::new();
    write_delta_frame(&mut frame, 3, 9, 0, 1, &chunk);
    assert!(
        fresh.insert_frame(&frame, 0).is_some(),
        "a reset assembler accepts the seq the new stream starts with"
    );
}
