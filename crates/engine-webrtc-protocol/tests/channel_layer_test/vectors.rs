use engine_webrtc_protocol::MAX_FRAME;
use engine_webrtc_protocol::channel::{
    ACK_WIRE_LEN, Ack, AckKind, DELTA_HEADER_LEN, DeltaHeader, FRAGMENT_TTL_MS, MAX_DELTA_BODY,
    MAX_DELTA_CHUNK, MAX_FRAGMENTS, MAX_INFLIGHT_BYTES, MAX_INFLIGHT_DELTAS, OP_ACK, OP_DELTA,
    delta_fragment_count, parse_ack, write_ack, write_delta_frame,
};

/// seq 1027, tick 65539, one frame carrying `[0xA5 01 02 03 FE]`.
pub(crate) const DELTA_SINGLE: [u8; 14] = [
    0x04, 0x03, 0x04, 0x03, 0x00, 0x01, 0x00, 0x00, 0x01, 0xa5, 0x01, 0x02, 0x03, 0xfe,
];

/// seq 65535, tick 8, chunk 0 of 2.
pub(crate) const DELTA_FRAG_0: [u8; 13] = [
    0x04, 0xff, 0xff, 0x08, 0x00, 0x00, 0x00, 0x00, 0x02, 0xde, 0xad, 0xbe, 0xef,
];

/// The same delta's chunk 1 of 2.
pub(crate) const DELTA_FRAG_1: [u8; 13] = [
    0x04, 0xff, 0xff, 0x08, 0x00, 0x00, 0x00, 0x01, 0x02, 0x00, 0x11, 0x22, 0x33,
];

/// Confirmation that event seq 91 arrived.
const ACK_EVENT: [u8; 4] = [0x06, 0x5b, 0x00, 0x01];

/// seq 65534, tick 0x00FF_FF01, fragment 0 of 3 — both stamps straddle a wrap.
const DELTA_WRAP_FRAG: [u8; 11] = [
    0x04, 0xfe, 0xff, 0x01, 0xff, 0xff, 0x00, 0x00, 0x03, 0x01, 0x02,
];

#[test]
fn rust_agrees_with_the_browser_on_delta_bytes() {
    let (header, chunk) = DeltaHeader::parse(&DELTA_SINGLE).expect("vector parses");
    assert_eq!(
        header,
        DeltaHeader {
            seq: 1027,
            tick: 65539,
            frag_idx: 0,
            frag_total: 1
        }
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

    write_delta_frame(&mut out, 65534, 0x00FF_FF01, 0, 3, &[0x01, 0x02]);
    assert_eq!(out, DELTA_WRAP_FRAG);
    let (header, chunk) = DeltaHeader::parse(&DELTA_WRAP_FRAG).expect("vector parses");
    assert_eq!((header.seq, header.tick), (65534, 0x00FF_FF01));
    assert_eq!(chunk, &[0x01, 0x02]);
}

#[test]
fn rust_agrees_with_the_browser_on_ack_bytes() {
    assert_eq!(
        parse_ack(&ACK_EVENT),
        Some(Ack {
            seq: 91,
            kind: AckKind::Event
        })
    );

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
    assert_eq!(MAX_FRAGMENTS, 255);
    assert_eq!(MAX_DELTA_BODY, MAX_DELTA_CHUNK * 255);
    assert_eq!(MAX_INFLIGHT_DELTAS, 8);
    assert_eq!(MAX_INFLIGHT_BYTES, 1 << 20);
}

#[test]
fn fragment_count_matches_the_chunk_size() {
    assert_eq!(delta_fragment_count(0), Some(1));
    assert_eq!(delta_fragment_count(1), Some(1));
    assert_eq!(delta_fragment_count(MAX_DELTA_CHUNK), Some(1));
    assert_eq!(delta_fragment_count(MAX_DELTA_CHUNK + 1), Some(2));
    assert_eq!(delta_fragment_count(MAX_DELTA_CHUNK * 254 + 1), Some(255));
    assert_eq!(delta_fragment_count(MAX_DELTA_BODY), Some(255));
    assert_eq!(delta_fragment_count(MAX_DELTA_BODY + 1), None);
}

/// First-byte dispatch is the SDK's only router, so 0x04 and 0x06 must be free
/// of every mesh and voice opcode.
#[test]
fn a_delta_frame_never_collides_with_the_mesh_opcodes() {
    for other in [0x01u8, 0x02, 0x03, 0x11, 0x18, 0x56, 0x57, 0xa5] {
        assert_ne!(OP_DELTA, other);
        assert_ne!(OP_ACK, other);
    }
    assert_eq!(MAX_DELTA_CHUNK + DELTA_HEADER_LEN, MAX_FRAME);
}
