use super::{FrameError, HEADER_LEN, frame_length};
use crate::bytes::{le_f32, le_u16, put_f32, put_u16};

pub const OP_EVENT: u8 = 0x03;

/// What happened. Small, rare, and the only frame the mesh must not lose.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum EventKind {
    Fire = 1,
    Hit = 2,
    Damage = 3,
    Kill = 4,
    Respawn = 5,
    Effect = 6,
}

impl EventKind {
    pub fn from_u8(value: u8) -> Option<Self> {
        Some(match value {
            1 => Self::Fire,
            2 => Self::Hit,
            3 => Self::Damage,
            4 => Self::Kill,
            5 => Self::Respawn,
            6 => Self::Effect,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EventFrame {
    pub seq: u16,
    pub kind: EventKind,
    pub from: u16,
    pub target: u16,
    pub tick: u16,
    pub value: f32,
}

/// Event frame is 14 bytes: header + kind + three ids + one float.
pub const EVENT_WIRE_LEN: usize = HEADER_LEN + 1 + 2 + 2 + 2 + 4;

impl EventFrame {
    pub fn decode(buf: &[u8]) -> Result<Self, FrameError> {
        if buf.len() != EVENT_WIRE_LEN {
            return Err(frame_length(EVENT_WIRE_LEN, buf.len()));
        }
        Ok(Self {
            seq: le_u16(buf, 1),
            kind: EventKind::from_u8(buf[3]).ok_or(FrameError::UnknownEventKind(buf[3]))?,
            from: le_u16(buf, 4),
            target: le_u16(buf, 6),
            tick: le_u16(buf, 8),
            value: le_f32(buf, 10),
        })
    }
}

/// Write an event frame into `out`, starting a fresh frame.
pub fn encode_event(frame: &EventFrame, out: &mut Vec<u8>) {
    out.clear();
    out.reserve(EVENT_WIRE_LEN);
    out.push(OP_EVENT);
    put_u16(out, frame.seq);
    out.push(frame.kind as u8);
    put_u16(out, frame.from);
    put_u16(out, frame.target);
    put_u16(out, frame.tick);
    put_f32(out, frame.value);
}
