use engine_webrtc_protocol::{
    EVENT_WIRE_LEN, EventFrame, EventKind, FrameError, MeshFrame, OP_EVENT, OP_VOICE, Opcode,
    decode, encode_event, header, is_newer,
};

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

/// The same buffer a client hands to sendVoice() on the WebSocket relay must be
/// valid on the mesh without a header rewrite.
#[test]
fn voice_rides_the_relay_bytes_untouched() {
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
