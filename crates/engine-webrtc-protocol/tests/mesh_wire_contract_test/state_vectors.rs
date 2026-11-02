use engine_webrtc_protocol::{
    EntitySnapshot, MeshFrame, OP_STATE, STATE_RECORD_LEN, decode, encode_state,
};

use crate::harness::{STATE_ANGLES, STATE_EXACT, assert_header};

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
