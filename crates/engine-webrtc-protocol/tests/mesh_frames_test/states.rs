use crate::snapshot;
use engine_webrtc_protocol::{
    EntitySnapshot, FrameError, LEN_SCALE, MAX_ENTITIES_PER_STATE, MeshFrame, OP_STATE,
    STATE_RECORD_LEN, decode, encode_state,
};

#[test]
fn state_records_stay_within_one_quant_step() {
    let mut wire = Vec::new();
    encode_state(4242, &[snapshot(7), snapshot(9)], &mut wire).unwrap();
    assert_eq!(wire.len(), 4 + 2 * STATE_RECORD_LEN);
    assert_eq!(wire[0], OP_STATE);
    assert_eq!(wire[3], 2, "entity count");

    let MeshFrame::State(frame) = decode(&wire).unwrap() else {
        panic!("expected a state frame");
    };
    assert_eq!(frame.tick, 4242);
    assert_eq!(frame.len(), 2);
    let got: Vec<EntitySnapshot> = frame.entities().collect();
    assert_eq!(got.len(), 2);

    let want = snapshot(7);
    assert_eq!(got[0].id, want.id);
    assert_eq!(got[0].state, want.state);
    let step = 1.0 / LEN_SCALE;
    for (a, b) in [
        (got[0].x, want.x),
        (got[0].y, want.y),
        (got[0].z, want.z),
        (got[0].vx, want.vx),
        (got[0].vy, want.vy),
    ] {
        assert!((a - b).abs() <= step, "{a} vs {b} exceeds one quant step");
    }
    assert_eq!(got[0].z, 499.9375);
    assert!(
        (got[0].yaw - want.yaw).abs() <= 0.001,
        "{} vs {}",
        got[0].yaw,
        want.yaw
    );
    assert!((got[0].pitch - want.pitch).abs() <= 0.001);
    assert_eq!(got[1].vy, -3.25);
}

#[test]
fn wild_floats_saturate_instead_of_poisoning_the_frame() {
    let far = EntitySnapshot {
        id: 1,
        x: 1.0e9,
        y: f32::MIN,
        z: f32::NAN,
        ..snapshot(1)
    };
    let mut wire = Vec::new();
    encode_state(1, &[far], &mut wire).unwrap();
    let MeshFrame::State(frame) = decode(&wire).unwrap() else {
        panic!()
    };
    let back = frame.entities().next().unwrap();
    assert_eq!(back.x, i16::MAX as f32 / LEN_SCALE);
    assert_eq!(back.y, i16::MIN as f32 / LEN_SCALE);
    assert_eq!(back.z, 0.0, "a non-finite length must not become garbage");
}

#[test]
fn state_frame_boundaries_are_enforced() {
    let mut wire = Vec::new();
    assert!(matches!(
        encode_state(1, &[], &mut wire),
        Err(FrameError::EmptyState)
    ));

    let too_many = vec![snapshot(3); MAX_ENTITIES_PER_STATE + 1];
    assert!(matches!(
        encode_state(1, &too_many, &mut wire),
        Err(FrameError::TooManyEntities { max, got }) if got == too_many.len() && max == MAX_ENTITIES_PER_STATE
    ));

    encode_state(9, &[snapshot(3)], &mut wire).unwrap();
    assert!(matches!(
        decode(&wire[..wire.len() - 1]),
        Err(FrameError::LengthMismatch { .. })
    ));
    wire.push(0xAA);
    assert!(matches!(
        decode(&wire),
        Err(FrameError::LengthMismatch { .. })
    ));

    let mut empty_count = vec![OP_STATE, 0x01, 0x00, 0x00];
    assert!(matches!(decode(&empty_count), Err(FrameError::EmptyState)));
    empty_count[3] = 4;
    assert!(matches!(
        decode(&empty_count),
        Err(FrameError::LengthMismatch { want, got } ) if want == 4 + 4 * STATE_RECORD_LEN && got == 4
    ));
}
