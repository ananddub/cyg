//! Cross-language wire contract. Every byte vector here was produced by the browser
//! SDK (`examples/tank-3d/openoxide.js`, v1.3.0) in Node, and is pinned verbatim. If
//! Rust and JS ever disagree about the layout, this test fails instead of two clients
//! silently reading each other's positions wrong.
//!
//! Regenerate with the values below using `node test-client/mesh_vectors.cjs`.

use engine_webrtc_protocol::{
    EntitySnapshot, EventFrame, EventKind, InputFrame, MeshFrame, OP_EVENT, OP_INPUT, OP_STATE,
    STATE_RECORD_LEN, decode, encode_event, encode_input, encode_state, header,
};

/// `[op, seq, flags, aimYaw, aimPitch, moveX, moveZ]` from
/// `encodeInputFrame({seq:513, flags:0xDEADBEEF, aimYaw:-1.25, aimPitch:0.5, moveX:-1, moveZ:0.25})`.
const INPUT_EXACT: [u8; 23] = [
    0x01, 0x01, 0x02, 0xef, 0xbe, 0xad, 0xde, 0x00, 0x00, 0xa0, 0xbf, 0x00, 0x00, 0x00, 0x3f, 0x00,
    0x00, 0x80, 0xbf, 0x00, 0x00, 0x80, 0x3e,
];

/// Two records on tick 4660, every field on the 1/16 grid and both angles zero, so the
/// bytes are exact in float32 and float64 alike.
const STATE_EXACT: [u8; 42] = [
    0x02, 0x34, 0x12, 0x02, 0x03, 0x00, 0xc8, 0x00, 0xf8, 0xff, 0x3f, 0x1f, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x0a, 0xcc, 0xff, 0x00, 0x00, 0x05, 0x09, 0x00, 0x00, 0x80, 0xff, 0x7f, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x80, 0x00, 0x00, 0xff, 0x7f, 0x01,
];

/// One record on tick 7 with yaw 1.0 rad and pitch -0.75 rad. The angle fields scale by
/// 2^15/tau, which is not a power of two, so JS (float64) and Rust (float32) may land one
/// LSB apart; everything else is byte-exact.
const STATE_ANGLES: [u8; 23] = [
    0x02, 0x07, 0x00, 0x01, 0x07, 0x00, 0x5f, 0xfe, 0x28, 0x00, 0xff, 0x1f, 0xbe, 0x28, 0x71, 0xe1,
    0x38, 0xff, 0x00, 0x00, 0xa0, 0x0f, 0x05,
];

/// DAMAGE from player 3 with no target, on tick 5001.
const EVENT_EXACT: [u8; 14] = [
    0x03, 0x5b, 0x00, 0x03, 0x03, 0x00, 0xff, 0xff, 0x89, 0x13, 0x00, 0x00, 0x0a, 0x42,
];

fn assert_header(wire: &[u8], want_op: u8, want_seq: u16) {
    let (op, seq) = header(wire).expect("vector has a readable header");
    assert_eq!(op as u8, want_op);
    assert_eq!(seq, want_seq);
}

#[test]
fn rust_agrees_with_the_browser_on_input_bytes() {
    assert_header(&INPUT_EXACT, OP_INPUT, 513);

    let MeshFrame::Input(frame) = decode(&INPUT_EXACT).unwrap() else {
        panic!("expected an input frame");
    };
    assert_eq!(
        frame,
        InputFrame {
            seq: 513,
            flags: 0xDEAD_BEEF,
            aim_yaw: -1.25,
            aim_pitch: 0.5,
            move_x: -1.0,
            move_z: 0.25,
        }
    );

    let mut wire = Vec::new();
    encode_input(&frame, &mut wire);
    assert_eq!(wire, INPUT_EXACT, "re-encode must be byte-identical to JS");
}

#[test]
fn rust_agrees_with_the_browser_on_state_bytes() {
    assert_header(&STATE_EXACT, OP_STATE, 4660);

    let MeshFrame::State(state) = decode(&STATE_EXACT).unwrap() else {
        panic!("expected a state frame");
    };
    assert_eq!(state.tick, 4660);
    assert_eq!(state.len(), 2);
    let entities: Vec<EntitySnapshot> = state.entities().collect();
    assert_eq!(
        entities,
        vec![
            EntitySnapshot {
                id: 3,
                x: 12.5,
                y: -0.5,
                z: 499.9375,
                yaw: 0.0,
                pitch: 0.0,
                vx: 160.0,
                vy: -3.25,
                vz: 0.0,
                state: 5,
            },
            EntitySnapshot {
                id: 9,
                x: -2048.0,
                y: 2047.9375,
                z: 0.0,
                yaw: 0.0,
                pitch: 0.0,
                vx: -2048.0,
                vy: 0.0,
                vz: 2047.9375,
                state: 1,
            },
        ]
    );

    let mut wire = Vec::new();
    encode_state(state.tick, &entities, &mut wire).unwrap();
    assert_eq!(wire, STATE_EXACT, "re-encode must be byte-identical to JS");
}

#[test]
fn rust_agrees_with_the_browser_on_event_bytes() {
    assert_header(&EVENT_EXACT, OP_EVENT, 91);

    let MeshFrame::Event(frame) = decode(&EVENT_EXACT).unwrap() else {
        panic!("expected an event frame");
    };
    assert_eq!(
        frame,
        EventFrame {
            seq: 91,
            kind: EventKind::Damage,
            from: 3,
            target: u16::MAX,
            tick: 5001,
            value: 34.5,
        }
    );

    let mut wire = Vec::new();
    encode_event(&frame, &mut wire);
    assert_eq!(wire, EVENT_EXACT, "re-encode must be byte-identical to JS");
}

#[test]
fn angle_fields_agree_within_one_lsb_and_the_rest_exactly() {
    let record = &STATE_ANGLES[4..];
    assert_eq!(record.len(), STATE_RECORD_LEN);

    let MeshFrame::State(state) = decode(&STATE_ANGLES).unwrap() else {
        panic!("expected a state frame");
    };
    let entities: Vec<EntitySnapshot> = state.entities().collect();
    let mine = entities[0];

    let mut wire = Vec::new();
    encode_state(state.tick, &entities, &mut wire).unwrap();
    let mine_record = &wire[4..];

    // Fields on the 1/16 grid must match byte for byte; the two angle slots may drift,
    // so they are compared as integers below instead.
    for at in [0, 2, 4, 6, 12, 14, 16, 18] {
        let n = if at == 18 { 1 } else { 2 };
        assert_eq!(
            &mine_record[at..at + n],
            &record[at..at + n],
            "record byte {at} disagrees with JS"
        );
    }

    let raw = |bytes: &[u8], at: usize| i16::from_le_bytes([bytes[at], bytes[at + 1]]);
    for at in [8, 10] {
        let drift = raw(mine_record, at).abs_diff(raw(record, at));
        assert!(drift <= 1, "angle field at {at} drifted {drift} LSB");
    }
    assert!((mine.yaw - 1.0).abs() < 1e-4, "yaw = {}", mine.yaw);
    assert!((mine.pitch + 0.75).abs() < 1e-4, "pitch = {}", mine.pitch);
    assert_eq!(mine.state, 0b101);
}
