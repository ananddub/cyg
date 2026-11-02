use crate::{feed, frame, vectors::DELTA_SINGLE};
use engine_webrtc_protocol::MAX_FRAME;
use engine_webrtc_protocol::channel::{
    DELTA_HEADER_LEN, DeltaHeader, FragmentAssembler, MAX_DELTA_CHUNK, OP_DELTA, parse_ack,
};

#[test]
fn malformed_headers_are_rejected_not_misparsed() {
    let mut assembler = FragmentAssembler::default();

    assert!(DeltaHeader::parse(&frame(1, 1, 0, 0, &[0xa5])).is_none());
    assert!(DeltaHeader::parse(&frame(1, 1, 2, 2, &[0xa5])).is_none());
    assert!(DeltaHeader::parse(&[OP_DELTA, 0x01, 0x02]).is_none());

    let mut foreign = frame(1, 1, 0, 1, &[0xa5]);
    foreign[0] = 0x02;
    assert!(DeltaHeader::parse(&foreign).is_none());

    assert!(
        DeltaHeader::parse(
            &[
                &DELTA_SINGLE[..DELTA_HEADER_LEN],
                &[0u8; MAX_DELTA_CHUNK + 1]
            ]
            .concat()
        )
        .is_none()
    );

    assert_eq!(parse_ack(&[0x06, 0x01, 0x00, 0x02]), None);
    assert_eq!(parse_ack(&[0x06, 0x01]), None);
    assert_eq!(parse_ack(&DELTA_SINGLE), None);

    assert_eq!(assembler.insert_frame(&[0x04, 0x01], 0), None);
    assert_eq!(assembler.insert_frame(&foreign, 0), None);
    assert_eq!(assembler.rejected(), 2);
    assert_eq!(assembler.assembled(), 0);

    let bogus = DeltaHeader {
        seq: 5,
        tick: 5,
        frag_idx: 9,
        frag_total: 2,
    };
    assert_eq!(assembler.insert(&bogus, &[0xa5], 0), None);
    let bogus_zero = DeltaHeader {
        seq: 5,
        tick: 5,
        frag_idx: 0,
        frag_total: 0,
    };
    assert_eq!(assembler.insert(&bogus_zero, &[0xa5], 0), None);
    assert_eq!(assembler.rejected(), 4);
}

#[test]
fn one_seq_cannot_claim_two_geometries() {
    let mut assembler = FragmentAssembler::default();
    assert_eq!(
        assembler.insert_frame(&frame(20, 1, 0, 2, &[0xa5]), 0),
        None
    );
    assert_eq!(
        assembler.insert_frame(&frame(20, 1, 0, 3, &[0xa5]), 1),
        None
    );
    assert_eq!(assembler.rejected(), 1);
    assert_eq!(assembler.inflight(), 0, "the conflicting slot is released");
}

#[test]
fn a_whole_mtu_of_body_splits_and_joins_byte_identical() {
    let body: Vec<u8> = (0..5000u32).map(|i| (i % 251) as u8).collect();
    let total = engine_webrtc_protocol::delta_fragment_count(body.len()).unwrap();
    assert_eq!(total, 5);

    let mut assembler = FragmentAssembler::default();
    let mut joined = None;
    for (idx, chunk) in body.chunks(MAX_DELTA_CHUNK).enumerate() {
        let wire = frame(42, 1000, idx as u8, total, chunk);
        assert!(wire.len() <= MAX_FRAME, "a fragment must fit one packet");
        if idx % 2 == 1 {
            joined = feed(&mut assembler, &wire, idx as u64);
        }
    }
    assert!(joined.is_none(), "the delta must not close early");
    for (idx, chunk) in body.chunks(MAX_DELTA_CHUNK).enumerate() {
        if idx % 2 == 0 {
            let wire = frame(42, 1000, idx as u8, total, chunk);
            joined = feed(&mut assembler, &wire, idx as u64);
        }
    }
    assert_eq!(joined, Some(body));
}
