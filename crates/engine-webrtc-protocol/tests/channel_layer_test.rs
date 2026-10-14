//! The star channel: how a server delta reaches a browser over `game-unreliable`.
//!
//! Every `[u8; N]` vector here was printed by the browser SDK with
//! `node test-client/channel_vectors.cjs` and is pinned verbatim, so a layout change on
//! one side fails a test rather than silently desynchronising two implementations.
//!
//! The behavioural half covers what a 10 % loss rate actually does: a superseded delta is
//! dropped unread, fragments reassemble in any order, and a delta that stops making
//! progress is forgotten instead of pinning its chunks.

use engine_webrtc_protocol::channel::{
    ACK_WIRE_LEN, Ack, AckKind, DELTA_HEADER_LEN, FRAGMENT_TTL_MS, MAX_DELTA_BODY,
    MAX_DELTA_CHUNK, OP_ACK, OP_DELTA, DeltaHeader, FragmentAssembler, delta_fragment_count,
    parse_ack, write_ack, write_delta_frame,
};

/// seq 1027, tick 65539, one frame carrying `[0xA5 01 02 03 FE]`.
const DELTA_SINGLE: [u8; 14] = [
    0x04, 0x03, 0x04, 0x03, 0x00, 0x01, 0x00, 0x00, 0x01, 0xa5, 0x01, 0x02, 0x03, 0xfe,
];

/// seq 65535, tick 8, chunk 0 of 2.
const DELTA_FRAG_0: [u8; 13] = [
    0x04, 0xff, 0xff, 0x08, 0x00, 0x00, 0x00, 0x00, 0x02, 0xde, 0xad, 0xbe, 0xef,
];

/// The same delta's chunk 1 of 2.
const DELTA_FRAG_1: [u8; 13] = [
    0x04, 0xff, 0xff, 0x08, 0x00, 0x00, 0x00, 0x01, 0x02, 0x00, 0x11, 0x22, 0x33,
];

/// Confirmation that event seq 91 arrived.
const ACK_EVENT: [u8; 4] = [0x06, 0x5b, 0x00, 0x01];

#[test]
fn rust_agrees_with_the_browser_on_delta_bytes() {
    let (header, chunk) = DeltaHeader::parse(&DELTA_SINGLE).expect("vector parses");
    assert_eq!(
        header,
        DeltaHeader { seq: 1027, tick: 65539, frag_idx: 0, frag_total: 1 }
    );
    assert_eq!(chunk, &[0xa5, 0x01, 0x02, 0x03, 0xfe]);
    assert!(header.is_single());

    let mut out = Vec::new();
    write_delta_frame(&mut out, 1027, 65539, 0, 1, chunk);
    assert_eq!(out, DELTA_SINGLE);
}

#[test]
fn rust_agrees_with_the_browser_on_fragment_bytes() {
    for (wire, idx) in [(&DELTA_FRAG_0, 0u8), (&DELTA_FRAG_1, 1)] {
        let (header, _) = DeltaHeader::parse(wire).expect("vector parses");
        assert_eq!(header.seq, 65535);
        assert_eq!(header.tick, 8);
        assert_eq!(header.frag_idx, idx);
        assert_eq!(header.frag_total, 2);
    }

    let mut out = Vec::new();
    write_delta_frame(&mut out, 65535, 8, 0, 2, &[0xde, 0xad, 0xbe, 0xef]);
    assert_eq!(out, DELTA_FRAG_0);
    write_delta_frame(&mut out, 65535, 8, 1, 2, &[0x00, 0x11, 0x22, 0x33]);
    assert_eq!(out, DELTA_FRAG_1);
}

#[test]
fn rust_agrees_with_the_browser_on_ack_bytes() {
    assert_eq!(parse_ack(&ACK_EVENT), Some(Ack { seq: 91, kind: AckKind::Event }));

    let mut out = Vec::new();
    write_ack(&mut out, 91, AckKind::Event);
    assert_eq!(out, ACK_EVENT);
}

#[test]
fn header_sizes_match_the_wire() {
    assert_eq!(OP_DELTA, DELTA_SINGLE[0]);
    assert_eq!(OP_ACK, ACK_EVENT[0]);
    assert_eq!(DELTA_HEADER_LEN, 9);
    assert_eq!(ACK_WIRE_LEN, 4);
    assert_eq!(MAX_DELTA_CHUNK, 1191);
    assert_eq!(FRAGMENT_TTL_MS, 3000);
    assert_eq!(MAX_DELTA_BODY, MAX_DELTA_CHUNK * 255);
}

fn feed(assembler: &mut FragmentAssembler, wire: &[u8], now_ms: u64) -> Option<Vec<u8>> {
    let (header, chunk) = DeltaHeader::parse(wire)?;
    assembler.insert(&header, chunk, now_ms).map(|d| d.body)
}

#[test]
fn fragments_reassemble_in_any_order() {
    let mut assembler = FragmentAssembler::default();

    // Chunk 1 lands first: nothing to hand to the game yet.
    assert_eq!(feed(&mut assembler, &DELTA_FRAG_1, 0), None);

    // Chunk 0 closes the delta, and the body is in index order, not arrival order.
    let body = feed(&mut assembler, &DELTA_FRAG_0, 10);
    assert_eq!(body, Some(vec![0xde, 0xad, 0xbe, 0xef, 0x00, 0x11, 0x22, 0x33]));
    assert_eq!(assembler.assembled(), 1);
    assert_eq!(assembler.newest_seq(), Some(65535));
}

#[test]
fn a_duplicate_fragment_does_not_double_count() {
    let mut assembler = FragmentAssembler::default();
    assert_eq!(feed(&mut assembler, &DELTA_FRAG_0, 0), None);
    assert_eq!(feed(&mut assembler, &DELTA_FRAG_0, 1), None);
    let body = feed(&mut assembler, &DELTA_FRAG_1, 2);
    assert_eq!(body, Some(vec![0xde, 0xad, 0xbe, 0xef, 0x00, 0x11, 0x22, 0x33]));
}

#[test]
fn a_superseded_delta_is_dropped_unread() {
    let mut assembler = FragmentAssembler::default();
    let mut seq = 0u16;
    let mut emitted = 0;
    for _ in 0..1000 {
        seq = seq.wrapping_add(1);
        let mut wire = Vec::new();
        write_delta_frame(&mut wire, seq, 0, 0, 1, &[0xa5]);
        if feed(&mut assembler, &wire, 0).is_some() {
            emitted += 1;
        }
        // The retransmit of the previous tick arriving late is always stale.
        let mut late = Vec::new();
        write_delta_frame(&mut late, seq.wrapping_sub(1), 0, 0, 1, &[0xa5]);
        assert_eq!(feed(&mut assembler, &late, 0), None);
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

    // 0 is one past 65535, so it is the newest thing on the channel.
    let mut next = Vec::new();
    write_delta_frame(&mut next, 0, 9, 0, 1, &[0xa5]);
    assert_eq!(feed(&mut assembler, &next, 0), Some(vec![0xa5]));

    // And 65534, which a naive unsigned compare calls "bigger", is stale.
    let mut behind = Vec::new();
    write_delta_frame(&mut behind, 65534, 9, 0, 1, &[0xa5]);
    assert_eq!(feed(&mut assembler, &behind, 0), None);
}

#[test]
fn a_stale_half_delta_is_cleaned_up_after_the_ttl() {
    let mut assembler = FragmentAssembler::default();
    assert_eq!(feed(&mut assembler, &DELTA_FRAG_0, 0), None);

    assert!(!assembler.cleanup(0));
    assert!(!assembler.cleanup(FRAGMENT_TTL_MS - 1));
    assert!(assembler.cleanup(FRAGMENT_TTL_MS + 1));
    assert_eq!(assembler.newest_seq(), None);

    // The slot is free, so the same delta can be built from scratch again.
    assert_eq!(feed(&mut assembler, &DELTA_FRAG_1, FRAGMENT_TTL_MS + 2), None);
    assert_eq!(
        feed(&mut assembler, &DELTA_FRAG_0, FRAGMENT_TTL_MS + 3),
        Some(vec![0xde, 0xad, 0xbe, 0xef, 0x00, 0x11, 0x22, 0x33])
    );
}

#[test]
fn a_newer_fragment_supersedes_the_one_being_built() {
    let mut assembler = FragmentAssembler::default();
    assert_eq!(feed(&mut assembler, &DELTA_FRAG_0, 0), None);

    let mut newer = Vec::new();
    write_delta_frame(&mut newer, 1028, 7, 0, 3, &[0x01]);
    assert_eq!(feed(&mut assembler, &newer, 1), None);

    // Chunk 1 of the old delta no longer belongs to anything.
    assert_eq!(feed(&mut assembler, &DELTA_FRAG_1, 2), None);
    assert_eq!(assembler.assembled(), 0);
}

#[test]
fn malformed_headers_are_rejected_not_misparsed() {
    let mut wire = Vec::new();

    // Zero fragments is not a delta.
    write_delta_frame(&mut wire, 1, 1, 0, 0, &[0xa5]);
    assert!(DeltaHeader::parse(&wire).is_none());

    // An index at or past the count can never close the message.
    write_delta_frame(&mut wire, 1, 1, 2, 2, &[0xa5]);
    assert!(DeltaHeader::parse(&wire).is_none());

    // Truncated below the header.
    assert!(DeltaHeader::parse(&[OP_DELTA, 0x01, 0x02]).is_none());

    // A frame that is not ours at all.
    write_delta_frame(&mut wire, 1, 1, 0, 1, &[0xa5]);
    wire[0] = 0x02;
    assert!(DeltaHeader::parse(&wire).is_none());

    // An ack with an unknown kind or the wrong length.
    assert_eq!(parse_ack(&[0x06, 0x01, 0x00, 0x02]), None);
    assert_eq!(parse_ack(&[0x06, 0x01]), None);
    assert_eq!(parse_ack(&DELTA_SINGLE), None);
}

#[test]
fn fragment_count_matches_the_chunk_size() {
    assert_eq!(delta_fragment_count(0), Some(1));
    assert_eq!(delta_fragment_count(1), Some(1));
    assert_eq!(delta_fragment_count(MAX_DELTA_CHUNK), Some(1));
    assert_eq!(delta_fragment_count(MAX_DELTA_CHUNK + 1), Some(2));
    assert_eq!(delta_fragment_count(MAX_DELTA_CHUNK * 254 + 1), Some(255));
    // Past 255 fragments there is no index left to put in the header.
    assert_eq!(delta_fragment_count(MAX_DELTA_BODY + 1), None);
}

#[test]
fn a_whole_mtu_of_body_splits_and_joins_byte_identical() {
    let body: Vec<u8> = (0..5000u32).map(|i| (i % 251) as u8).collect();
    let total = delta_fragment_count(body.len()).unwrap();
    assert_eq!(total, 5);

    let mut assembler = FragmentAssembler::default();
    let mut joined = None;
    for (idx, chunk) in body.chunks(MAX_DELTA_CHUNK).enumerate() {
        let mut wire = Vec::new();
        write_delta_frame(&mut wire, 42, 1000, idx as u8, total, chunk);
        assert!(wire.len() <= engine_webrtc_protocol::MAX_FRAME, "a fragment must fit one packet");
        // Reverse-ish arrival: odd chunks first.
        if idx % 2 == 1 {
            joined = feed(&mut assembler, &wire, idx as u64);
        }
    }
    assert!(joined.is_none(), "the delta must not close early");
    for (idx, chunk) in body.chunks(MAX_DELTA_CHUNK).enumerate() {
        if idx % 2 == 0 {
            let mut wire = Vec::new();
            write_delta_frame(&mut wire, 42, 1000, idx as u8, total, chunk);
            joined = feed(&mut assembler, &wire, idx as u64);
        }
    }
    assert_eq!(joined, Some(body));
}

#[test]
fn a_delta_frame_never_collides_with_the_mesh_opcodes() {
    // First-byte dispatch is the SDK's only router, so 0x04 and 0x06 must be free.
    for other in [0x01u8, 0x02, 0x03, 0x11, 0x18, 0x56, 0x57, 0xa5] {
        assert_ne!(OP_DELTA, other);
        assert_ne!(OP_ACK, other);
    }
}
