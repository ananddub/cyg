//! The star channel: how a server delta reaches a browser over `game-unreliable`.
//!
//! Every `[u8; N]` vector here was printed by the browser SDK with
//! `node test-client/channel_vectors.cjs` (and, for the packer-only vectors, by the
//! SDK's `encodeDeltaFrame` directly) and is pinned verbatim, so a layout change on
//! one side fails a test rather than silently desynchronising two implementations.
//!
//! The behavioural half covers what a 10 % loss rate actually does: a superseded delta is
//! dropped unread, fragments reassemble in any order, several deltas stay in flight at
//! once, and a delta that stops making progress is forgotten instead of pinning its chunks.
//! The loss/reorder simulation lives in `channel_loss_sim_test.rs`.

use engine_webrtc_protocol::channel::{
    ACK_WIRE_LEN, DELTA_HEADER_LEN, FRAGMENT_TTL_MS, MAX_DELTA_BODY, MAX_DELTA_CHUNK,
    MAX_FRAGMENTS, MAX_INFLIGHT_BYTES, MAX_INFLIGHT_DELTAS, OP_ACK, OP_DELTA, Ack, AckKind,
    DeltaHeader, DeltaPacker, FragmentAssembler, PackError, delta_fragment_count, is_newer,
    is_superseded, pack_delta, pack_delta_frames, parse_ack, write_ack, write_delta_frame,
};
use engine_webrtc_protocol::MAX_FRAME;

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

/// Header of fragment 0 of a 1194-byte body: seq 777, tick 99999, split 2 ways by the
/// packer. Pinned from `encodeDeltaFrame(777, 99999, 0, 2, <1191 bytes>)`.
const PACKED_FRAG_0_HEADER: [u8; DELTA_HEADER_LEN] =
    [0x04, 0x09, 0x03, 0x9f, 0x86, 0x01, 0x00, 0x00, 0x02];

/// Fragment 1 of the same delta, whole: the tail chunk is `[0x42 0x43 0x44]`.
const PACKED_FRAG_1: [u8; 12] = [
    0x04, 0x09, 0x03, 0x9f, 0x86, 0x01, 0x00, 0x01, 0x02, 0x42, 0x43, 0x44,
];

/// seq 65534, tick 0x00FF_FF01, fragment 0 of 3 — both stamps straddle a wrap, so a
/// byte-order or width mistake cannot hide.
const DELTA_WRAP_FRAG: [u8; 11] = [
    0x04, 0xfe, 0xff, 0x01, 0xff, 0xff, 0x00, 0x00, 0x03, 0x01, 0x02,
];

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

    // A fragment that straddles the u16 sequence wrap and the u32 tick wrap.
    write_delta_frame(&mut out, 65534, 0x00FF_FF01, 0, 3, &[0x01, 0x02]);
    assert_eq!(out, DELTA_WRAP_FRAG);
    let (header, chunk) = DeltaHeader::parse(&DELTA_WRAP_FRAG).expect("vector parses");
    assert_eq!((header.seq, header.tick), (65534, 0x00FF_FF01));
    assert_eq!(chunk, &[0x01, 0x02]);
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
    assert_eq!(MAX_FRAGMENTS, 255);
    assert_eq!(MAX_DELTA_BODY, MAX_DELTA_CHUNK * 255);
    assert_eq!(MAX_INFLIGHT_DELTAS, 8);
    assert_eq!(MAX_INFLIGHT_BYTES, 1 << 20);
}

fn feed(assembler: &mut FragmentAssembler, wire: &[u8], now_ms: u64) -> Option<Vec<u8>> {
    let (header, chunk) = DeltaHeader::parse(wire)?;
    assembler.insert(&header, chunk, now_ms).map(|d| d.body)
}

fn frame(seq: u16, tick: u32, idx: u8, total: u8, chunk: &[u8]) -> Vec<u8> {
    let mut wire = Vec::new();
    write_delta_frame(&mut wire, seq, tick, idx, total, chunk);
    wire
}

/// The half-range rule, in both directions, and the fact that "superseded" is its exact
/// complement — so the server's filter and the client's can never disagree about which
/// delta wins.
#[test]
fn sequence_wrap_is_one_rule_both_ends_share() {
    assert!(is_newer(0, 1), "one past is newer");
    assert!(!is_newer(1, 1), "equal is not newer: a delta applies once");
    assert!(!is_newer(1, 0), "one behind is older");
    assert!(is_newer(u16::MAX, 0), "the forward wrap is newer");
    assert!(!is_newer(0, u16::MAX), "and reading it backwards is the classic bug");
    assert!(is_newer(65530, 5), "a 511-tick gap across the wrap is still newer");
    assert!(!is_newer(5, 65530), "while the same gap in reverse is stale");
    assert!(!is_newer(0, 0x8000), "the half-range boundary is not newer");

    for (last, seq) in [(0u16, 1u16), (1, 0), (u16::MAX, 0), (0, u16::MAX), (7, 7)] {
        assert_eq!(is_superseded(last, seq), !is_newer(last, seq));
        assert!(is_superseded(last, last), "a duplicate is superseded");
    }
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
    assert_eq!(assembler.inflight(), 0);
    assert_eq!(assembler.buffered_bytes(), 0);
}

#[test]
fn a_duplicate_fragment_does_not_double_count() {
    let mut assembler = FragmentAssembler::default();
    assert_eq!(feed(&mut assembler, &DELTA_FRAG_0, 0), None);
    assert_eq!(feed(&mut assembler, &DELTA_FRAG_0, 1), None);
    assert_eq!(assembler.duplicates(), 1);
    assert_eq!(assembler.buffered_bytes(), 4, "the duplicate was not stored twice");
    let body = feed(&mut assembler, &DELTA_FRAG_1, 2);
    assert_eq!(body, Some(vec![0xde, 0xad, 0xbe, 0xef, 0x00, 0x11, 0x22, 0x33]));
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
        // The retransmit of the previous tick arriving late is always stale.
        let late = frame(seq.wrapping_sub(1), 0, 0, 1, &[0xa5]);
        assert_eq!(feed(&mut assembler, &late, 0), None);
        // And the receiver can ask the same question without touching the table.
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

    // 0 is one past 65535, so it is the newest thing on the channel.
    let next = frame(0, 9, 0, 1, &[0xa5]);
    assert_eq!(feed(&mut assembler, &next, 0), Some(vec![0xa5]));

    // And 65534, which a naive unsigned compare calls "bigger", is stale.
    let behind = frame(65534, 9, 0, 1, &[0xa5]);
    assert_eq!(feed(&mut assembler, &behind, 0), None);

    // A fragmented delta wraps the same way: 1 is newer than 0, 65530 is not.
    let ahead = frame(1, 9, 0, 2, &[0xa5]);
    let gap = frame(65530, 9, 0, 2, &[0x5a]);
    assert_eq!(feed(&mut assembler, &gap, 0), None);
    assert_eq!(assembler.stale(), 2);
    assert_eq!(feed(&mut assembler, &ahead, 0), None);
    let close = frame(1, 9, 1, 2, &[0xa6]);
    assert_eq!(feed(&mut assembler, &close, 0), Some(vec![0xa5, 0xa6]));
}

#[test]
fn a_stale_half_delta_is_cleaned_up_after_the_ttl() {
    let mut assembler = FragmentAssembler::default();
    assert_eq!(feed(&mut assembler, &DELTA_FRAG_0, 0), None);

    assert!(!assembler.cleanup(0));
    assert!(!assembler.cleanup(FRAGMENT_TTL_MS - 1));
    assert!(assembler.cleanup(FRAGMENT_TTL_MS + 1));
    assert_eq!(assembler.newest_seq(), None);
    assert_eq!(assembler.inflight(), 0);
    assert_eq!(assembler.buffered_bytes(), 0, "an expired slot returns its bytes");

    // The slot is free, so the same delta can be built from scratch again.
    assert_eq!(feed(&mut assembler, &DELTA_FRAG_1, FRAGMENT_TTL_MS + 2), None);
    assert_eq!(
        feed(&mut assembler, &DELTA_FRAG_0, FRAGMENT_TTL_MS + 3),
        Some(vec![0xde, 0xad, 0xbe, 0xef, 0x00, 0x11, 0x22, 0x33])
    );
}

/// TTL is per delta: a peer that keeps making progress on one tick must not lose the
/// stalled one next to it, and vice versa.
#[test]
fn ttl_expiry_sweeps_only_the_stalled_deltas() {
    let mut assembler = FragmentAssembler::default();
    assert_eq!(assembler.insert_frame(&frame(100, 10, 0, 3, &[1]), 0), None);
    assert_eq!(assembler.insert_frame(&frame(101, 11, 0, 3, &[2]), 0), None);
    assert_eq!(assembler.inflight(), 2);

    // 101 keeps moving; 100 goes quiet.
    assert_eq!(assembler.insert_frame(&frame(101, 11, 1, 3, &[3]), 2_500), None);
    assert_eq!(assembler.assembled(), 0, "neither delta has all three chunks yet");

    assert!(assembler.cleanup(3_501));
    assert_eq!(assembler.inflight(), 1);
    assert_eq!(assembler.newest_seq(), None);
    assert_eq!(assembler.buffered_bytes(), 2, "only the stalled delta returned its bytes");
    assert!(!assembler.cleanup(4_000), "the live slot is not stalled");

    // The live delta still closes, and the dead one has to start over.
    assert_eq!(assembler.insert_frame(&frame(101, 11, 2, 3, &[4]), 4_500).unwrap().body, vec![2, 3, 4]);
    assert_eq!(assembler.newest_seq(), Some(101));
    assert_eq!(assembler.insert_frame(&frame(100, 10, 0, 3, &[1]), 4_600), None);
    assert_eq!(assembler.stale(), 1, "100 is behind what was delivered");
}

/// Interleaving is the point of the unreliable channel: the tail of one delta arrives
/// inside the next one, and both must still close byte-exactly.
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
        let chunk = if *seq == 10 { &parts_a[*idx] } else { &parts_b[*idx] };
        // One tick per delta: the stamp is the delta's, not the fragment's.
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

/// An in-flight delta that a delivery left behind is released immediately: it can never
/// be applied, and holding it is the leak this layer exists to avoid.
#[test]
fn a_delivery_releases_the_older_partial() {
    let mut assembler = FragmentAssembler::default();
    assert_eq!(assembler.insert_frame(&frame(10, 1, 0, 2, &[0xaa]), 0), None);
    assert_eq!(assembler.inflight(), 1);

    assert_eq!(assembler.insert_frame(&frame(11, 2, 0, 2, &[0xbb]), 1), None);
    assert_eq!(assembler.inflight(), 2, "both deltas are in flight");

    let delta = assembler
        .insert_frame(&frame(11, 2, 1, 2, &[0xbc]), 2)
        .expect("11 closes");
    assert_eq!((delta.seq, delta.tick, delta.body), (11, 2, vec![0xbb, 0xbc]));
    assert_eq!(assembler.inflight(), 0, "10 was superseded by the delivery");
    assert_eq!(assembler.evicted(), 1);
    assert_eq!(assembler.buffered_bytes(), 0);

    // Its surviving chunk now has no delta to belong to.
    assert_eq!(assembler.insert_frame(&frame(10, 1, 1, 2, &[0xab]), 3), None);
    assert_eq!(assembler.stale(), 1);
    assert_eq!(assembler.assembled(), 1);
}

/// The window is bounded, because a peer can open a hundred deltas with one fragment
/// each and the server pays per connection.
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

    // Eviction dropped the *oldest*, so the newest delta still closes.
    let last = 39u16;
    let delta = assembler
        .insert_frame(&frame(last, last as u32, 1, 2, &[0x42]), 100)
        .expect("the newest delta survives the window");
    assert_eq!(delta.seq, last);
    let mut expected = vec![0x41u8; MAX_DELTA_CHUNK];
    expected.push(0x42);
    assert_eq!(delta.body, expected);

    // A delta that lost its slot to eviction has to start over.
    assert_eq!(assembler.insert_frame(&frame(0, 0, 1, 2, &[0x42]), 101), None);
    assert_eq!(assembler.stale(), 1, "0 is behind what has been delivered");
}

#[test]
fn malformed_headers_are_rejected_not_misparsed() {
    let mut assembler = FragmentAssembler::default();

    // Zero fragments is not a delta.
    assert!(DeltaHeader::parse(&frame(1, 1, 0, 0, &[0xa5])).is_none());

    // An index at or past the count can never close the message.
    assert!(DeltaHeader::parse(&frame(1, 1, 2, 2, &[0xa5])).is_none());

    // Truncated below the header.
    assert!(DeltaHeader::parse(&[OP_DELTA, 0x01, 0x02]).is_none());

    // A frame that is not ours at all.
    let mut foreign = frame(1, 1, 0, 1, &[0xa5]);
    foreign[0] = 0x02;
    assert!(DeltaHeader::parse(&foreign).is_none());

    // A chunk that could not have fitted in one packet was fragmented by SCTP, so it
    // is not a delta frame this codec produced.
    assert!(
        DeltaHeader::parse(&[&DELTA_SINGLE[..DELTA_HEADER_LEN], &[0u8; MAX_DELTA_CHUNK + 1]].concat())
            .is_none()
    );

    // An ack with an unknown kind or the wrong length.
    assert_eq!(parse_ack(&[0x06, 0x01, 0x00, 0x02]), None);
    assert_eq!(parse_ack(&[0x06, 0x01]), None);
    assert_eq!(parse_ack(&DELTA_SINGLE), None);

    // `insert_frame` counts what it refuses instead of misassembling it.
    assert_eq!(assembler.insert_frame(&[0x04, 0x01], 0), None);
    assert_eq!(assembler.insert_frame(&foreign, 0), None);
    assert_eq!(assembler.rejected(), 2);
    assert_eq!(assembler.assembled(), 0);

    // A hand-built header with impossible geometry is refused too, not indexed into.
    let bogus = DeltaHeader { seq: 5, tick: 5, frag_idx: 9, frag_total: 2 };
    assert_eq!(assembler.insert(&bogus, &[0xa5], 0), None);
    let bogus_zero = DeltaHeader { seq: 5, tick: 5, frag_idx: 0, frag_total: 0 };
    assert_eq!(assembler.insert(&bogus_zero, &[0xa5], 0), None);
    assert_eq!(assembler.rejected(), 4);
}

/// One seq must describe one body: a second geometry for it is refused, because joining
/// the two would hand the game a delta that belongs to neither sender.
#[test]
fn one_seq_cannot_claim_two_geometries() {
    let mut assembler = FragmentAssembler::default();
    assert_eq!(assembler.insert_frame(&frame(20, 1, 0, 2, &[0xa5]), 0), None);
    assert_eq!(assembler.insert_frame(&frame(20, 1, 0, 3, &[0xa5]), 1), None);
    assert_eq!(assembler.rejected(), 1);
    assert_eq!(assembler.inflight(), 0, "the conflicting slot is released");
}

#[test]
fn fragment_count_matches_the_chunk_size() {
    assert_eq!(delta_fragment_count(0), Some(1));
    assert_eq!(delta_fragment_count(1), Some(1));
    assert_eq!(delta_fragment_count(MAX_DELTA_CHUNK), Some(1));
    assert_eq!(delta_fragment_count(MAX_DELTA_CHUNK + 1), Some(2));
    assert_eq!(delta_fragment_count(MAX_DELTA_CHUNK * 254 + 1), Some(255));
    // Past 255 fragments there is no index left to put in the header.
    assert_eq!(delta_fragment_count(MAX_DELTA_BODY), Some(255));
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
        let wire = frame(42, 1000, idx as u8, total, chunk);
        assert!(wire.len() <= MAX_FRAME, "a fragment must fit one packet");
        // Reverse-ish arrival: odd chunks first.
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

#[test]
fn a_delta_frame_never_collides_with_the_mesh_opcodes() {
    // First-byte dispatch is the SDK's only router, so 0x04 and 0x06 must be free.
    for other in [0x01u8, 0x02, 0x03, 0x11, 0x18, 0x56, 0x57, 0xa5] {
        assert_ne!(OP_DELTA, other);
        assert_ne!(OP_ACK, other);
    }
}

/// The packer's bytes are the SDK's bytes: `MAX_DELTA_CHUNK` body per fragment, a short
/// tail, and a full 1200-byte frame that does not spill.
#[test]
fn the_packer_emits_the_pinned_fragment_vectors() {
    let mut body = vec![0x41u8; MAX_DELTA_CHUNK];
    body.extend_from_slice(&[0x42, 0x43, 0x44]);

    let packing = pack_delta(777, 99999, &body).expect("1194 bytes is two fragments");
    assert_eq!((packing.seq(), packing.tick(), packing.frag_total()), (777, 99999, 2));
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
    assert_eq!(out.len(), MAX_FRAME, "a full chunk must land exactly on the MTU");
    assert_eq!(&out[..DELTA_HEADER_LEN], &PACKED_FRAG_0_HEADER[..]);
    assert!(out[DELTA_HEADER_LEN..].iter().all(|&b| b == 0x41));

    assert_eq!(fragments[1].to_vec(), PACKED_FRAG_1);
    assert_eq!(pack_delta_frames(777, 99999, &body).unwrap()[1], PACKED_FRAG_1);
}

/// Every fragment the packer lists carries the same seq/tick, an index that covers the
/// body exactly, and the assembler puts it back byte for byte.
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
    // Feed the tail first: the newest delta must still close in index order.
    let mut joined = None;
    for (idx, frame) in frames.iter().enumerate().rev() {
        joined = assembler.insert_frame(frame, idx as u64).map(|d| d.body);
        if idx > 0 {
            assert!(joined.is_none(), "a delta closes on its last chunk, not its first");
        }
    }
    assert_eq!(joined, Some(body));
    assert_eq!(assembler.assembled(), 1);
    assert_eq!(assembler.inflight(), 0);
}

/// More than `MAX_FRAGMENTS` chunks is not a delta, and the codec says so instead of
/// truncating — a half-sent delta is a corrupt world state.
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

    // The boundary itself is carryable.
    let fits = vec![0u8; MAX_DELTA_BODY];
    assert_eq!(pack_delta(1, 1, &fits).unwrap().frag_total(), 255);
}

/// The send side mints one room-wide seq per delta and stores nothing: there is no queue
/// to resend from, and a refused delta does not consume a seq, so receivers never have to
/// reason about a gap.
#[test]
fn the_packer_holds_the_room_sequence_across_the_wrap() {
    let mut packer = DeltaPacker::new(u16::MAX - 1, 100);
    assert_eq!(packer.next_seq(), 65534);
    assert_eq!(packer.tick(), 100);

    let first = packer.pack(&[1, 2, 3]).unwrap();
    assert_eq!(first.seq(), 65534);
    assert_eq!(packer.next_seq(), 65535);
    assert_eq!(packer.pack(&[4, 5]).unwrap().seq(), 65535);
    assert_eq!(packer.next_seq(), 0, "the sequence wraps with the room, not against it");
    assert_eq!(packer.published(), 2);

    packer.set_tick(103);
    let wrapped = packer.pack(&[6]).unwrap();
    assert_eq!((wrapped.seq(), wrapped.tick()), (0, 103));
    assert_eq!(packer.next_seq(), 1);
    assert_eq!(packer.published(), 3);
    assert_eq!(packer.refused(), 0);

    // A body the channel cannot carry is refused without spending a seq.
    let too_big = vec![0u8; MAX_DELTA_BODY + 1];
    assert!(packer.pack(&too_big).is_err());
    assert_eq!(packer.next_seq(), 1, "a failed pack did not advance the sequence");
    assert!(packer.pack_or_drop(&too_big).is_none());
    assert_eq!(packer.refused(), 1);
    assert_eq!(packer.next_seq(), 1, "pack_or_drop does not either");
    assert!(packer.pack_or_drop(&[7]).is_some());
    assert_eq!(packer.published(), 4);
    assert_eq!(packer.next_seq(), 2);
}

/// Deltas are newest-wins and never resent: a seq at or behind what the receiver holds is
/// one compare to detect, and the receiver never asks for the delta it dropped.
#[test]
fn a_newest_wins_receiver_never_asks_for_the_delta_it_dropped() {
    let body: Vec<u8> = (0..4000u32).map(|i| i as u8).collect();
    let late = pack_delta_frames(50, 3000, &body).unwrap();
    let current = pack_delta_frames(51, 3001, &body).unwrap();

    let mut assembler = FragmentAssembler::default();
    // 51 completes first; 50's fragments then arrive, whole and in order.
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
    assert_eq!(assembler.assembled(), before, "a superseded delta cannot apply twice");
    assert_eq!(assembler.inflight(), 0);
}
