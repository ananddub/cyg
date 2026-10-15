use engine_webrtc_protocol::{
    FrameError, INPUT_WIRE_LEN, InputFrame, MeshFrame, OP_INPUT, decode, encode_input,
};

#[test]
fn input_frame_is_exact_and_bit_preserving() {
    let frame = InputFrame {
        seq: 60_000,
        flags: 0xDEAD_BEEF,
        aim_yaw: -1.2345,
        aim_pitch: 0.5,
        move_x: -1.0,
        move_z: 0.25,
    };
    let mut wire = Vec::new();
    encode_input(&frame, &mut wire);
    assert_eq!(wire.len(), INPUT_WIRE_LEN);
    assert_eq!(wire[0], OP_INPUT);

    let MeshFrame::Input(back) = decode(&wire).unwrap() else {
        panic!("expected an input frame");
    };
    assert_eq!(back.seq, frame.seq);
    assert_eq!(back.flags, frame.flags);
    assert!(back.aim_yaw.to_bits() == frame.aim_yaw.to_bits());
    assert!(back.move_z.to_bits() == frame.move_z.to_bits());
}

#[test]
fn truncated_frames_are_rejected_not_misread() {
    let frame = InputFrame {
        seq: 1,
        flags: 2,
        aim_yaw: 0.0,
        aim_pitch: 0.0,
        move_x: 0.0,
        move_z: 0.0,
    };
    let mut wire = Vec::new();
    encode_input(&frame, &mut wire);

    for len in 0..wire.len() {
        let result = decode(&wire[..len]);
        assert!(
            matches!(
                result,
                Err(FrameError::LengthMismatch { .. } | FrameError::TooShort { .. })
            ),
            "length {len} decoded as {result:?}"
        );
    }
    let mut long = wire.clone();
    long.push(0);
    assert!(matches!(
        decode(&long),
        Err(FrameError::LengthMismatch { want, got }) if want == INPUT_WIRE_LEN && got == long.len()
    ));
}

#[test]
fn unknown_opcodes_are_refused() {
    for op in [0x00u8, 0x04, 0x55, 0x57, 0xA5, 0xFF] {
        let buf = [op, 0, 0];
        assert!(
            matches!(decode(&buf), Err(FrameError::UnknownOpcode(v)) if v == op),
            "opcode 0x{op:02X} must not decode"
        );
    }
    assert!(matches!(
        decode(&[]),
        Err(FrameError::TooShort { need: 1, got: 0 })
    ));
}
