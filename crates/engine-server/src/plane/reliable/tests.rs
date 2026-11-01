use super::*;
use engine_webrtc_protocol::{EventFrame, EventKind, OP_EVENT, encode_event};

pub(super) fn wire(seq: u16) -> Vec<u8> {
    let mut out = Vec::new();
    encode_event(
        &EventFrame {
            seq,
            kind: EventKind::Kill,
            from: 3,
            target: u16::MAX,
            tick: 77,
            value: 1.0,
        },
        &mut out,
    );
    assert_eq!(out.len(), MAX_EVENT_BYTES);
    assert_eq!(out[0], OP_EVENT);
    out
}

#[derive(Default)]
pub(super) struct Sink {
    pub(super) sent: Vec<u16>,
    pub(super) accept: bool,
}

impl Sink {
    pub(super) fn take(&mut self, bytes: &[u8]) -> bool {
        if !self.accept {
            return false;
        }
        self.sent
            .push(EventFrame::decode(bytes).expect("event wire").seq);
        true
    }
}

mod ack;
mod ack_limits;
mod order;
