//! Wire-contract tests for the DataChannel mesh frames: exact layouts, boundary
//! rejection, quantisation error, and the newest-wins rule both ends share.

use engine_webrtc_protocol::{
    EVENT_WIRE_LEN, EntitySnapshot, EventFrame, EventKind, FrameError, INPUT_WIRE_LEN, InputFrame,
    LEN_SCALE, MAX_ENTITIES_PER_STATE, MeshFrame, OP_EVENT, OP_INPUT, OP_STATE, OP_VOICE, Opcode,
    STATE_RECORD_LEN, decode, encode_event, encode_input, encode_state, header, is_newer,
};

fn snapshot(id: u16) -> EntitySnapshot {
    EntitySnapshot {
        id,
        x: 12.34,
        y: -0.5,
        z: 499.9375,
        yaw: -2.5,
        pitch: 0.75,
        vx: 160.0,
        vy: -3.25,
        vz: 0.0,
        state: 0b101,
    }
}

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
    // Floats ride raw, so prediction-relevant fields must be bit-identical.
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
    // z is exactly representable at 1/16 and must survive untouched.
    assert_eq!(got[0].z, 499.9375);
    assert!(
        (got[0].yaw - want.yaw).abs() <= 0.001,
        "{} vs {}",
        got[0].yaw,
        want.yaw
    );
    assert!((got[0].pitch - want.pitch).abs() <= 0.001);
    // A snapshot already at the quant grid decodes to itself.
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
    // A datagram holds exactly one frame: a short read or trailing bytes are bugs.
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

#[test]
fn events_are_reliable_and_typed() {
    let frame = EventFrame {
        seq: 12,
        kind: EventKind::Damage,
        from: 3,
        target: u16::MAX,
        tick: 5001,
        value: 34.5,
    };
    let mut wire = Vec::new();
    encode_event(&frame, &mut wire);
    assert_eq!(wire.len(), EVENT_WIRE_LEN);
    assert_eq!(wire[0], OP_EVENT);

    let MeshFrame::Event(back) = decode(&wire).unwrap() else {
        panic!()
    };
    assert_eq!(back, frame);
    assert_eq!(back.target, u16::MAX, "no-target events must survive");

    let mut bogus = wire.clone();
    bogus[3] = 200;
    assert!(matches!(
        decode(&bogus),
        Err(FrameError::UnknownEventKind(200))
    ));

    let (opcode, _) = header(&wire).unwrap();
    assert_eq!(opcode, Opcode::Event);
    assert!(opcode.is_reliable(), "events must ride the ordered channel");
    assert!(!Opcode::Input.is_reliable());
    assert!(!Opcode::State.is_reliable());
}

#[test]
fn voice_rides_the_relay_bytes_untouched() {
    // Same buffer a client hands to sendVoice() on the WebSocket relay.
    let mut packet = vec![OP_VOICE, 0, 0, 0, 0, 0, 7, 0, 0, 0, 2, 0, 0, 0, 0];
    packet.extend_from_slice(b"opus-payload");

    let MeshFrame::Voice(borrowed) = decode(&packet).unwrap() else {
        panic!("expected voice");
    };
    assert!(
        std::ptr::eq(borrowed.as_ptr(), packet.as_ptr()),
        "voice must be borrowed, never copied"
    );
    assert_eq!(borrowed.len(), packet.len());

    let (opcode, seq) = header(&packet).unwrap();
    assert_eq!(opcode, Opcode::Voice);
    assert_eq!(
        seq, 0,
        "voice carries its own sequence in the packet header"
    );
    assert!(!opcode.is_reliable());

    packet.truncate(14);
    assert!(matches!(
        decode(&packet),
        Err(FrameError::TooShort { need: 15, .. })
    ));
}

#[test]
fn newest_wins_survives_the_wrap() {
    assert!(is_newer(10, 11));
    assert!(!is_newer(11, 11), "a duplicate must not re-apply");
    assert!(!is_newer(11, 10), "a late retransmit must lose");
    assert!(is_newer(u16::MAX, 1), "sequence must survive the u16 wrap");
    assert!(is_newer(65_530, 5));
    assert!(
        !is_newer(0, 0x8000),
        "half the space apart is ambiguous, so treat it as old"
    );
}
