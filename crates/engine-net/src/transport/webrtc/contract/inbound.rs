//! Classifying a client frame against the lane it arrived on.

use engine_webrtc_protocol::{
    ACK_WIRE_LEN, Ack, EVENT_WIRE_LEN, EventFrame, HEADER_LEN, INPUT_WIRE_LEN, OP_ACK, OP_DELTA,
    OP_EVENT, OP_INPUT, parse_ack,
};

use super::INPUT_PAYLOAD_LEN;
use super::lane::Lane;

/// Why a frame was refused rather than handed to the game.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RejectReason {
    /// The channel it arrived on is not part of the contract.
    OffContract,
    /// A frame whose opcode belongs to the other lane. Reliability is a
    /// property of the lane, so an ack that arrived unreliated is not an ack.
    WrongLane { expected: Lane, got: Lane },
    /// The opcode is known but the length is not one legal wire frame.
    Malformed { op: u8, got: usize, want: usize },
}

/// A client frame classified against its lane.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Inbound {
    /// 50 Hz–128 Hz movement input. `resupplied` means the high-water mark had
    /// already passed this seq: the host may still want it for rollback replay,
    /// so it is flagged rather than thrown away.
    Input {
        seq: u16,
        payload: [u8; INPUT_PAYLOAD_LEN],
        resupplied: bool,
    },
    /// A gameplay event on the ordered lane. The server stamps its own seq when
    /// it fans this out, so the raw frame is passed through untouched.
    Event { raw: Vec<u8> },
    /// Proof the client holds a reliable frame.
    Ack(Ack),
    /// A frame the transport understands the lane for but does not model.
    Other { op: u8, len: usize },
    /// A typed app message (SDK `sendBytes`): the FULL wire frame
    /// `[0x05][type_len u8][type][payload]` — the host strips the envelope
    /// before handing bytes to the guest's `on_input`.
    App { data: Vec<u8> },
    /// Refused, with the reason counted by the caller.
    Refused(RejectReason),
}

/// Decode an `InputFrame` body: the 20 bytes after `[op][seq u16]`, exactly as
/// the host's `apply_input` expects them.
pub fn input_body(data: &[u8]) -> Option<[u8; INPUT_PAYLOAD_LEN]> {
    let rest = data.get(HEADER_LEN..)?;
    rest.get(..INPUT_PAYLOAD_LEN)
        .map(|body| body.try_into().expect("slice of fixed length"))
}

/// Classify a frame that arrived on `lane` (`None` = a channel outside the
/// contract).
///
/// Lane enforcement is what makes the reliability mapping explicit: a 4-byte
/// ack pattern on the unreliable lane is refused instead of being believed,
/// because SCTP could have dropped it and the server would then believe an
/// event was delivered that never arrived.
pub fn classify_inbound(lane: Option<Lane>, data: &[u8]) -> Inbound {
    let Some(lane) = lane else {
        return Inbound::Refused(RejectReason::OffContract);
    };
    let Some(&op) = data.first() else {
        return Inbound::Refused(RejectReason::Malformed {
            op: 0,
            got: 0,
            want: INPUT_WIRE_LEN,
        });
    };

    match op {
        OP_INPUT => {
            if data.len() != INPUT_WIRE_LEN {
                return malformed(op, data.len(), INPUT_WIRE_LEN);
            }
            if lane != Lane::Unreliable {
                return wrong_lane(lane, Lane::Unreliable);
            }
            let seq = u16::from_le_bytes([data[1], data[2]]);
            match input_body(data) {
                Some(payload) => Inbound::Input {
                    seq,
                    payload,
                    resupplied: false,
                },
                None => malformed(op, data.len(), INPUT_WIRE_LEN),
            }
        }
        OP_ACK => match parse_ack(data) {
            Some(ack) if lane == Lane::Reliable => Inbound::Ack(ack),
            Some(_) => wrong_lane(lane, Lane::Reliable),
            None => malformed(op, data.len(), ACK_WIRE_LEN),
        },
        OP_EVENT => {
            if lane != Lane::Reliable {
                return wrong_lane(lane, Lane::Reliable);
            }
            match EventFrame::decode(data) {
                Ok(_) => Inbound::Event { raw: data.to_vec() },
                Err(_) => malformed(op, data.len(), EVENT_WIRE_LEN),
            }
        }
        // A client echoing a server delta back is never valid input.
        OP_DELTA => malformed(op, data.len(), INPUT_WIRE_LEN),
        // SDK `sendBytes` typed app messages ride the reliable lane as
        // [0x05][type_len][type][payload] — hand the full envelope to the
        // host, which strips the envelope per game.
        0x05 => {
            if data.len() < 2 {
                malformed(op, data.len(), 2)
            } else {
                Inbound::App {
                    data: data.to_vec(),
                }
            }
        }
        other => Inbound::Other {
            op: other,
            len: data.len(),
        },
    }
}

fn wrong_lane(got: Lane, expected: Lane) -> Inbound {
    Inbound::Refused(RejectReason::WrongLane { expected, got })
}

fn malformed(op: u8, got: usize, want: usize) -> Inbound {
    Inbound::Refused(RejectReason::Malformed { op, got, want })
}
