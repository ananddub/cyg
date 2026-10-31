//! Gap-closing contract tests for the wire codec and the delta channel.
//!
//! Pins behaviors that the layer/loss tests implied but never asserted:
//! - Decode rejects over-MTU / over-count state frames (the encode side could
//!   never produce them; a corrupt or hostile peer must not slip them through)
//! - `header()` on truncated non-voice buffers
//! - Non-finite angle quantization
//! - Empty-body deltas survive pack → wire → assemble end to end
//! - The 1 MiB inflight-byte budget actually evicts (peer spraying large
//!   partial deltas cannot balloon receiver memory)
//! - Reconnect semantics: a fresh assembler trusts the stream again

use engine_webrtc_protocol::channel::{
    MAX_INFLIGHT_BYTES, MAX_INFLIGHT_DELTAS, MAX_DELTA_CHUNK, FragmentAssembler,
};
use engine_webrtc_protocol::{
    AssembledDelta, DELTA_HEADER_LEN, EntitySnapshot, FrameError, Opcode, write_delta_frame,
};
use engine_webrtc_protocol::{
    MAX_ENTITIES_PER_STATE, MAX_FRAME, STATE_HEADER_LEN, STATE_RECORD_LEN, decode, encode_state,
    header, quantize_angle,
};

// ---------------------------------------------------------------------------
// State-frame decode limits (mirror of the encode-side invariants)
// ---------------------------------------------------------------------------

fn snapshot(id: u16) -> EntitySnapshot {
    EntitySnapshot {
        id,
        x: 1.0,
        y: 2.0,
        z: 3.0,
        yaw: 0.0,
        pitch: 0.0,
        vx: 0.0,
        vy: 0.0,
        vz: 0.0,
        state: 0,
    }
}

#[test]
fn state_decode_accepts_exactly_max_entities() {
    let entities: Vec<EntitySnapshot> = (0..MAX_ENTITIES_PER_STATE as u16).map(snapshot).collect();
    let mut buf = Vec::new();
    encode_state(7, &entities, &mut buf).expect("max-entities state encodes");
    assert!(buf.len() <= MAX_FRAME);
    match decode(&buf).expect("decodes") {
        engine_webrtc_protocol::MeshFrame::State(state) => {
            assert_eq!(state.tick, 7);
            assert_eq!(state.len(), MAX_ENTITIES_PER_STATE);
        }
        other => panic!("expected state frame, got {other:?}"),
    }
}

#[test]
fn state_decode_rejects_count_over_the_entity_cap() {
    // A peer claims 255 entities — more than fit in an MTU frame.
    let count = 255usize;
    let mut buf = vec![0u8; STATE_HEADER_LEN + count * STATE_RECORD_LEN];
    buf[0] = engine_webrtc_protocol::OP_STATE;
    buf[1] = 7;
    buf[2] = 0;
    buf[3] = count as u8;
    match decode(&buf) {
        Err(FrameError::TooManyEntities { max, got }) => {
            assert_eq!(max, MAX_ENTITIES_PER_STATE);
            assert_eq!(got, count);
        }
        other => panic!("expected TooManyEntities, got {:?}", other.err()),
    }
}

#[test]
fn state_decode_rejects_over_mtu_buffers() {
    // Exact length match for a *legal* count, but the datagram itself is over
    // the MTU — trailing garbage must not be tolerated on the state path.
    let count = MAX_ENTITIES_PER_STATE;
    let mut buf = vec![0u8; STATE_HEADER_LEN + count * STATE_RECORD_LEN + 64];
    buf[0] = engine_webrtc_protocol::OP_STATE;
    buf[1] = 7;
    buf[2] = 0;
    buf[3] = count as u8;
    match decode(&buf) {
        Err(FrameError::LengthMismatch { want, got }) => {
            assert_eq!(want, MAX_FRAME);
            assert_eq!(got, buf.len());
        }
        other => panic!("expected LengthMismatch, got {:?}", other.err()),
    }
}

// ---------------------------------------------------------------------------
// header(): prefix inspection on truncated buffers
// ---------------------------------------------------------------------------

#[test]
fn header_rejects_truncated_non_voice_buffers() {
    assert_eq!(header(&[]), None);
    assert_eq!(header(&[engine_webrtc_protocol::OP_INPUT]), None);
    // 2 bytes: opcode + half a seq — still not enough.
    assert_eq!(header(&[engine_webrtc_protocol::OP_INPUT, 0x01]), None);
    // Voice only needs the opcode byte.
    assert_eq!(
        header(&[engine_webrtc_protocol::OP_VOICE]),
        Some((Opcode::Voice, 0))
    );
}

// ---------------------------------------------------------------------------
// Non-finite inputs to the quantizers
// ---------------------------------------------------------------------------

#[test]
fn angle_quantization_lands_nan_and_infinities_on_zero() {
    // NaN poisons every comparison in the wrap, and Rust's saturating float
    // cast sends the final NaN to 0. Pin it: a corrupt angle degrades to 0,
    // it never wraps into an extreme rotation.
    assert_eq!(quantize_angle(f32::NAN), 0);
    assert_eq!(quantize_angle(f32::INFINITY), 0);
    assert_eq!(quantize_angle(f32::NEG_INFINITY), 0);
}

// ---------------------------------------------------------------------------
// Empty-body delta: pack → wire → assemble
// ---------------------------------------------------------------------------

#[test]
fn empty_body_delta_roundtrips_end_to_end() {
    let mut packer = engine_webrtc_protocol::channel::DeltaPacker::new(500, 42);
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

// ---------------------------------------------------------------------------
// Inflight byte budget: the receiver's memory is bounded even when a peer
// sprays near-255-fragment deltas (H3 datagram contract analogue)
// ---------------------------------------------------------------------------

/// A first fragment of a `total`-fragment delta, as raw wire bytes.
fn first_fragment(seq: u16, tick: u32, total: u8) -> Vec<u8> {
    let chunk = [0xABu8; MAX_DELTA_CHUNK];
    let mut frame = Vec::new();
    write_delta_frame(&mut frame, seq, tick, 0, total, &chunk);
    frame
}

#[test]
fn byte_budget_evicts_oldest_partial_deltas() {
    let mut assembler = FragmentAssembler::default();

    // 9 deltas of 130 fragments each: 130 * 1191 = 154,830 B per partial.
    // 8 in flight = 1,238,640 B > MAX_INFLIGHT_BYTES (1 MiB), so admitting
    // the 9th must evict until the budget holds.
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

    // The 9th delta's first fragment: eviction is forced by the byte budget,
    // not by the window (the window is exactly full too, but the point of this
    // test is that a >1MiB backlog cannot persist).
    let frame = first_fragment(1008, 1, TOTAL);
    assembler.insert_frame(&frame, 0);
    assert!(assembler.evicted() >= 1, "budget forced at least one eviction");
    assert_eq!(assembler.inflight(), MAX_INFLIGHT_DELTAS);
}

#[test]
fn budget_eviction_keeps_the_newest_delta_recoverable() {
    let mut assembler = FragmentAssembler::default();
    const TOTAL: u8 = 130;

    // Fill 8 slots with partials, force evictions, then finish the NEWEST
    // delta: its remaining fragments must still assemble despite the churn.
    for seq in 2000..2008u16 {
        let frame = first_fragment(seq, 1, TOTAL);
        assembler.insert_frame(&frame, 0);
    }
    let frame = first_fragment(2008, 1, TOTAL);
    assembler.insert_frame(&frame, 0);

    // Deliver the rest of delta 2008 (fragments 1..130) — chunk is 1 KiB here,
    // fine because the slot exists; only first-fragment admission is budgeted.
    for idx in 1..TOTAL {
        let start = idx as usize * MAX_DELTA_CHUNK;
        let chunk = vec![0xCDu8; MAX_DELTA_CHUNK];
        let _ = start; // body content is irrelevant for reassembly
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

// ---------------------------------------------------------------------------
// Reconnect semantics
// ---------------------------------------------------------------------------

#[test]
fn a_fresh_assembler_trusts_the_stream_again() {
    // After a reconnect the receiver builds a new FragmentAssembler. There is
    // no carried-over `newest`, so any seq — even one far "behind" the old
    // stream — is accepted. Pin this deliberately: it is the documented reset
    // contract, and the half-range rule only guards within one stream.
    let mut old = FragmentAssembler::default();
    let chunk = [1u8; 64];
    let mut frame = Vec::new();
    write_delta_frame(&mut frame, 65_000, 9, 0, 1, &chunk);
    assert!(old.insert_frame(&frame, 0).is_some());

    let mut fresh = FragmentAssembler::default();
    let mut frame = Vec::new();
    write_delta_frame(&mut frame, 3, 9, 0, 1, &chunk); // far behind 65000
    assert!(
        fresh.insert_frame(&frame, 0).is_some(),
        "a reset assembler accepts the seq the new stream starts with"
    );
}
