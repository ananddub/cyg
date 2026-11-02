use engine_webrtc_protocol::{
    EntitySnapshot, FrameError, MAX_ENTITIES_PER_STATE, MAX_FRAME, MeshFrame, Opcode,
    STATE_HEADER_LEN, STATE_RECORD_LEN, decode, encode_state, header, quantize_angle,
};

use crate::harness::snapshot;

#[test]
fn state_decode_accepts_exactly_max_entities() {
    let entities: Vec<EntitySnapshot> =
        (0..MAX_ENTITIES_PER_STATE as u16).map(snapshot).collect();
    let mut buf = Vec::new();
    encode_state(7, &entities, &mut buf).expect("max-entities state encodes");
    assert!(buf.len() <= MAX_FRAME);
    match decode(&buf).expect("decodes") {
        MeshFrame::State(state) => {
            assert_eq!(state.tick, 7);
            assert_eq!(state.len(), MAX_ENTITIES_PER_STATE);
        }
        other => panic!("expected state frame, got {other:?}"),
    }
}

#[test]
fn state_decode_rejects_count_over_the_entity_cap() {
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

#[test]
fn header_rejects_truncated_non_voice_buffers() {
    assert_eq!(header(&[]), None);
    assert_eq!(header(&[engine_webrtc_protocol::OP_INPUT]), None);

    assert_eq!(header(&[engine_webrtc_protocol::OP_INPUT, 0x01]), None);

    assert_eq!(
        header(&[engine_webrtc_protocol::OP_VOICE]),
        Some((Opcode::Voice, 0))
    );
}

#[test]
fn angle_quantization_lands_nan_and_infinities_on_zero() {
    assert_eq!(quantize_angle(f32::NAN), 0);
    assert_eq!(quantize_angle(f32::INFINITY), 0);
    assert_eq!(quantize_angle(f32::NEG_INFINITY), 0);
}
