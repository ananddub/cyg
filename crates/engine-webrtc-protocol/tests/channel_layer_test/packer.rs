use engine_webrtc_protocol::MAX_FRAME;
use engine_webrtc_protocol::channel::{
    DELTA_HEADER_LEN, DeltaPacker, FragmentAssembler, MAX_DELTA_BODY, MAX_DELTA_CHUNK,
    MAX_FRAGMENTS, PackError, pack_delta, pack_delta_frames,
};

/// Header of fragment 0 of a 1194-byte body: seq 777, tick 99999. Pinned from
/// `encodeDeltaFrame(777, 99999, 0, 2, <1191 bytes>)`.
const PACKED_FRAG_0_HEADER: [u8; DELTA_HEADER_LEN] =
    [0x04, 0x09, 0x03, 0x9f, 0x86, 0x01, 0x00, 0x00, 0x02];

/// Fragment 1 of the same delta, whole: the tail chunk is `[0x42 0x43 0x44]`.
const PACKED_FRAG_1: [u8; 12] = [
    0x04, 0x09, 0x03, 0x9f, 0x86, 0x01, 0x00, 0x01, 0x02, 0x42, 0x43, 0x44,
];

/// The packer's bytes are the SDK's bytes: a full `MAX_DELTA_CHUNK` fragment
/// lands exactly on the MTU and the short tail matches the pinned vector.
#[test]
fn the_packer_emits_the_pinned_fragment_vectors() {
    let mut body = vec![0x41u8; MAX_DELTA_CHUNK];
    body.extend_from_slice(&[0x42, 0x43, 0x44]);

    let packing = pack_delta(777, 99999, &body).expect("1194 bytes is two fragments");
    assert_eq!(
        (packing.seq(), packing.tick(), packing.frag_total()),
        (777, 99999, 2)
    );
    assert_eq!(packing.body_len(), body.len());
    assert_eq!(packing.wire_len(), DELTA_HEADER_LEN * 2 + body.len());

    let fragments: Vec<_> = packing.fragments().collect();
    assert_eq!(fragments.len(), 2);
    assert_eq!(fragments[0].header.frag_idx, 0);
    assert_eq!(fragments[0].chunk.len(), MAX_DELTA_CHUNK);
    assert_eq!(fragments[1].header.frag_idx, 1);
    assert_eq!(fragments[1].chunk, &[0x42, 0x43, 0x44]);

    let mut out = Vec::new();
    fragments[0].append_to(&mut out);
    assert_eq!(
        out.len(),
        MAX_FRAME,
        "a full chunk must land exactly on the MTU"
    );
    assert_eq!(&out[..DELTA_HEADER_LEN], &PACKED_FRAG_0_HEADER[..]);
    assert!(out[DELTA_HEADER_LEN..].iter().all(|&b| b == 0x41));

    assert_eq!(fragments[1].to_vec(), PACKED_FRAG_1);
    assert_eq!(
        pack_delta_frames(777, 99999, &body).unwrap()[1],
        PACKED_FRAG_1
    );
}

#[test]
fn the_packer_round_trips_through_the_assembler() {
    let body: Vec<u8> = (0..5000u32).map(|i| (i % 251) as u8).collect();
    let frames = pack_delta_frames(4242, 98_765, &body).expect("packs");
    assert_eq!(frames.len(), 5);

    let packing = pack_delta(4242, 98_765, &body).unwrap();
    let mut in_one_buffer = Vec::new();
    assert_eq!(packing.append_frames_to(&mut in_one_buffer), 5);
    assert_eq!(in_one_buffer.len(), packing.wire_len());
    assert_eq!(
        in_one_buffer
            .chunks(MAX_FRAME)
            .map(<[u8]>::to_vec)
            .collect::<Vec<_>>(),
        frames,
        "5 full fragments and a tail pack contiguously"
    );

    let mut assembler = FragmentAssembler::default();
    let mut joined = None;
    for (idx, frame) in frames.iter().enumerate().rev() {
        joined = assembler.insert_frame(frame, idx as u64).map(|d| d.body);
        if idx > 0 {
            assert!(
                joined.is_none(),
                "a delta closes on its last chunk, not its first"
            );
        }
    }
    assert_eq!(joined, Some(body));
    assert_eq!(assembler.assembled(), 1);
    assert_eq!(assembler.inflight(), 0);
}

/// More than `MAX_FRAGMENTS` chunks is not a delta, and the codec says so
/// instead of truncating — a half-sent delta is a corrupt world state.
#[test]
fn an_oversized_body_is_refused_not_truncated() {
    let body = vec![0u8; MAX_DELTA_BODY + 1];
    let error = pack_delta(1, 1, &body).expect_err("256 fragments cannot be indexed");
    assert_eq!(
        error,
        PackError::BodyTooLarge {
            body_len: MAX_DELTA_BODY + 1,
            fragments_needed: MAX_FRAGMENTS + 1,
            max_body_len: MAX_DELTA_BODY,
        }
    );
    assert!(error.to_string().contains("256 fragments"), "{error}");
    assert!(pack_delta_frames(1, 1, &body).is_err());

    let fits = vec![0u8; MAX_DELTA_BODY];
    assert_eq!(pack_delta(1, 1, &fits).unwrap().frag_total(), 255);
}

/// The send side mints one room-wide seq per delta and stores nothing: a
/// refused delta does not consume a seq, so receivers never reason about a gap.
#[test]
fn the_packer_holds_the_room_sequence_across_the_wrap() {
    let mut packer = DeltaPacker::new(u16::MAX - 1, 100);
    assert_eq!(packer.next_seq(), 65534);
    assert_eq!(packer.tick(), 100);

    let first = packer.pack(&[1, 2, 3]).unwrap();
    assert_eq!(first.seq(), 65534);
    assert_eq!(packer.next_seq(), 65535);
    assert_eq!(packer.pack(&[4, 5]).unwrap().seq(), 65535);
    assert_eq!(
        packer.next_seq(),
        0,
        "the sequence wraps with the room, not against it"
    );
    assert_eq!(packer.published(), 2);

    packer.set_tick(103);
    let wrapped = packer.pack(&[6]).unwrap();
    assert_eq!((wrapped.seq(), wrapped.tick()), (0, 103));
    assert_eq!(packer.next_seq(), 1);
    assert_eq!(packer.published(), 3);
    assert_eq!(packer.refused(), 0);

    let too_big = vec![0u8; MAX_DELTA_BODY + 1];
    assert!(packer.pack(&too_big).is_err());
    assert_eq!(
        packer.next_seq(),
        1,
        "a failed pack did not advance the sequence"
    );
    assert!(packer.pack_or_drop(&too_big).is_none());
    assert_eq!(packer.refused(), 1);
    assert_eq!(packer.next_seq(), 1, "pack_or_drop does not either");
    assert!(packer.pack_or_drop(&[7]).is_some());
    assert_eq!(packer.published(), 4);
    assert_eq!(packer.next_seq(), 2);
}

/// Deltas are newest-wins and never resent: a seq at or behind what the
/// receiver holds is one compare to detect, and the receiver never asks for it.
#[test]
fn a_newest_wins_receiver_never_asks_for_the_delta_it_dropped() {
    let body: Vec<u8> = (0..4000u32).map(|i| i as u8).collect();
    let late = pack_delta_frames(50, 3000, &body).unwrap();
    let current = pack_delta_frames(51, 3001, &body).unwrap();

    let mut assembler = FragmentAssembler::default();
    for frame in &current {
        assembler.insert_frame(frame, 0);
    }
    assert_eq!(assembler.assembled(), 1);
    assert!(assembler.is_superseded(50));

    let before = assembler.assembled();
    for frame in &late {
        assert_eq!(assembler.insert_frame(frame, 1), None);
    }
    assert_eq!(assembler.stale(), late.len() as u64);
    assert_eq!(
        assembler.assembled(),
        before,
        "a superseded delta cannot apply twice"
    );
    assert_eq!(assembler.inflight(), 0);
}
