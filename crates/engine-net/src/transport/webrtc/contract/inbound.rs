
use engine_webrtc_protocol::{
    ACK_WIRE_LEN, Ack, EVENT_WIRE_LEN, EventFrame, HEADER_LEN, INPUT_WIRE_LEN, OP_ACK, OP_DELTA,
    OP_EVENT, OP_INPUT, parse_ack,
};

use super::INPUT_PAYLOAD_LEN;
use super::lane::Lane;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RejectReason {

    OffContract,

    WrongLane { expected: Lane, got: Lane },

    Malformed { op: u8, got: usize, want: usize },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Inbound {

    Input {
        seq: u16,
        payload: [u8; INPUT_PAYLOAD_LEN],
        resupplied: bool,
    },

    Event { raw: Vec<u8> },

    Ack(Ack),

    Other { op: u8, len: usize },

    App { data: Vec<u8> },

    Refused(RejectReason),
}

pub fn input_body(data: &[u8]) -> Option<[u8; INPUT_PAYLOAD_LEN]> {
    let rest = data.get(HEADER_LEN..)?;
    rest.get(..INPUT_PAYLOAD_LEN)
        .map(|body| body.try_into().expect("slice of fixed length"))
}

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

        OP_DELTA => malformed(op, data.len(), INPUT_WIRE_LEN),

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
