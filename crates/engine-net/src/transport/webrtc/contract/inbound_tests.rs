//! Lane and opcode classification of one inbound frame.

use engine_webrtc_protocol::{
    ACK_WIRE_LEN, Ack, AckKind, EventFrame, EventKind, HEADER_LEN, INPUT_WIRE_LEN, OP_ACK,
    OP_DELTA, OP_INPUT, encode_event, parse_ack, write_ack,
};

use super::INPUT_PAYLOAD_LEN;
use super::fixture::frame;
use super::{Inbound, Lane, RejectReason, classify_inbound, input_body};

fn input(seq: u16) -> Vec<u8> {
    let mut buf = vec![OP_INPUT];
    buf.extend_from_slice(&seq.to_le_bytes());
    buf.resize(INPUT_WIRE_LEN, 0x11);
    buf
}

#[test]
fn ack_round_trips_and_is_length_strict() {
    let mut wire = Vec::new();
    write_ack(&mut wire, 91, AckKind::Event);
    assert_eq!(wire.len(), ACK_WIRE_LEN);
    assert_eq!(
        parse_ack(&wire),
        Some(Ack {
            seq: 91,
            kind: AckKind::Event
        })
    );
    assert_eq!(
        classify_inbound(Some(Lane::Reliable), &wire),
        Inbound::Ack(Ack {
            seq: 91,
            kind: AckKind::Event
        })
    );

    // A longer payload that merely starts with the ack opcode is not an ack.
    let mut padded = wire.clone();
    padded.push(0);
    assert!(parse_ack(&padded).is_none());
    assert!(matches!(
        classify_inbound(Some(Lane::Reliable), &padded),
        Inbound::Refused(RejectReason::Malformed { op: OP_ACK, .. })
    ));
    // An unknown ack kind is not an ack either.
    let mut bogus = wire.clone();
    bogus[3] = 7;
    assert!(parse_ack(&bogus).is_none());
}

#[test]
fn an_ack_on_the_wrong_lane_is_refused() {
    let mut wire = Vec::new();
    write_ack(&mut wire, 5, AckKind::Event);
    assert_eq!(
        classify_inbound(Some(Lane::Unreliable), &wire),
        Inbound::Refused(RejectReason::WrongLane {
            expected: Lane::Reliable,
            got: Lane::Unreliable
        })
    );
}

#[test]
fn inputs_are_parsed_on_the_unreliable_lane_only() {
    let buf = input(4);
    match classify_inbound(Some(Lane::Unreliable), &buf) {
        Inbound::Input {
            seq,
            payload,
            resupplied,
        } => {
            assert_eq!(seq, 4);
            assert!(!resupplied);
            assert_eq!(payload.as_slice(), &[0x11u8; INPUT_PAYLOAD_LEN]);
        }
        other => panic!("expected an input, got {other:?}"),
    }
    assert!(matches!(
        classify_inbound(Some(Lane::Reliable), &buf),
        Inbound::Refused(RejectReason::WrongLane {
            expected: Lane::Unreliable,
            got: Lane::Reliable
        })
    ));
    assert!(matches!(
        classify_inbound(None, &buf),
        Inbound::Refused(RejectReason::OffContract)
    ));
    // 22 bytes is not an input frame.
    assert!(matches!(
        classify_inbound(Some(Lane::Unreliable), &buf[..22]),
        Inbound::Refused(RejectReason::Malformed {
            op: OP_INPUT,
            got: 22,
            want: INPUT_WIRE_LEN
        })
    ));
}

#[test]
fn a_client_delta_frame_is_never_accepted() {
    let buf = frame(3, 4, 0, 1, &[9]);
    assert!(matches!(
        classify_inbound(Some(Lane::Unreliable), &buf),
        Inbound::Refused(RejectReason::Malformed { op: OP_DELTA, .. })
    ));
}

#[test]
fn events_are_accepted_on_the_reliable_lane_only() {
    let mut raw = Vec::new();
    let event = EventFrame {
        seq: 1,
        kind: EventKind::Fire,
        from: 3,
        target: 4,
        tick: 12,
        value: 1.0,
    };
    encode_event(&event, &mut raw);
    assert!(matches!(
        classify_inbound(Some(Lane::Reliable), &raw),
        Inbound::Event { .. }
    ));
    assert!(matches!(
        classify_inbound(Some(Lane::Unreliable), &raw),
        Inbound::Refused(RejectReason::WrongLane { .. })
    ));
}

#[test]
fn input_body_extraction_matches_the_host_payload_layout() {
    let buf = input(300);
    assert_eq!(
        input_body(&buf).as_ref().map(|b| &b[..]),
        Some(&buf[HEADER_LEN..])
    );
    assert_eq!(input_body(&buf[..10]), None);
}
