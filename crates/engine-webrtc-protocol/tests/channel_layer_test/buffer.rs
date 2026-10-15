use crate::{feed, frame, vectors::DELTA_SINGLE};
use engine_webrtc_protocol::MAX_FRAME;
use engine_webrtc_protocol::channel::{
    DELTA_HEADER_LEN, DeltaHeader, FragmentAssembler, MAX_DELTA_CHUNK, MAX_INFLIGHT_BYTES,
    MAX_INFLIGHT_DELTAS, OP_DELTA, parse_ack,
};

/// An in-flight delta that a delivery left behind is released immediately: it
/// can never be applied, and holding it is the leak this layer exists to avoid.
#[test]
fn a_delivery_releases_the_older_partial() {
    let mut assembler = FragmentAssembler::default();
    assert_eq!(
        assembler.insert_frame(&frame(10, 1, 0, 2, &[0xaa]), 0),
        None
    );
    assert_eq!(assembler.inflight(), 1);

    assert_eq!(
        assembler.insert_frame(&frame(11, 2, 0, 2, &[0xbb]), 1),
        None
    );
    assert_eq!(assembler.inflight(), 2, "both deltas are in flight");

    let delta = assembler
        .insert_frame(&frame(11, 2, 1, 2, &[0xbc]), 2)
        .expect("11 closes");
    assert_eq!(
        (delta.seq, delta.tick, delta.body),
        (11, 2, vec![0xbb, 0xbc])
    );
    assert_eq!(assembler.inflight(), 0, "10 was superseded by the delivery");
    assert_eq!(assembler.evicted(), 1);
    assert_eq!(assembler.buffered_bytes(), 0);

    assert_eq!(
        assembler.insert_frame(&frame(10, 1, 1, 2, &[0xab]), 3),
        None
    );
    assert_eq!(assembler.stale(), 1);
    assert_eq!(assembler.assembled(), 1);
}

/// The window is bounded, because a peer can open a hundred deltas with one
/// fragment each and the server pays per connection.
#[test]
fn the_inflight_window_bounds_a_spray_of_half_deltas() {
    let mut assembler = FragmentAssembler::default();
    for seq in 0..40u16 {
        let wire = frame(seq, seq as u32, 0, 2, &[0x41; MAX_DELTA_CHUNK]);
        assembler.insert_frame(&wire, seq as u64);
        assert!(
            assembler.inflight() <= MAX_INFLIGHT_DELTAS,
            "the window may not exceed {MAX_INFLIGHT_DELTAS} deltas"
        );
        assert!(assembler.buffered_bytes() <= MAX_INFLIGHT_BYTES);
    }
    assert_eq!(assembler.evicted(), 40 - MAX_INFLIGHT_DELTAS as u64);

    let last = 39u16;
    let delta = assembler
        .insert_frame(&frame(last, last as u32, 1, 2, &[0x42]), 100)
        .expect("the newest delta survives the window");
    assert_eq!(delta.seq, last);
    let mut expected = vec![0x41u8; MAX_DELTA_CHUNK];
    expected.push(0x42);
    assert_eq!(delta.body, expected);

    assert_eq!(
        assembler.insert_frame(&frame(0, 0, 1, 2, &[0x42]), 101),
        None
    );
    assert_eq!(assembler.stale(), 1, "0 is behind what has been delivered");
}

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

/// One seq must describe one body: a second geometry for it is refused, because
/// joining the two would hand the game a delta that belongs to neither sender.
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
