use engine_webrtc_protocol::{
    EventFrame, EventKind, InputFrame, MeshFrame, OP_EVENT, OP_INPUT, decode, encode_event,
    encode_input,
};

use crate::harness::{EVENT_EXACT, INPUT_EXACT, assert_header};

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
