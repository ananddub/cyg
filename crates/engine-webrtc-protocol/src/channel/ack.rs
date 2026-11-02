use crate::bytes::le_u16;

pub const OP_ACK: u8 = 0x06;

pub const ACK_WIRE_LEN: usize = 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum AckKind {
    Event = 1,
}

impl AckKind {
    pub fn from_u8(value: u8) -> Option<Self> {
        match value {
            1 => Some(Self::Event),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Ack {
    pub seq: u16,
    pub kind: AckKind,
}

pub fn write_ack(out: &mut Vec<u8>, seq: u16, kind: AckKind) {
    out.clear();
    out.reserve(ACK_WIRE_LEN);
    out.push(OP_ACK);
    out.extend_from_slice(&seq.to_le_bytes());
    out.push(kind as u8);
}

pub fn parse_ack(buf: &[u8]) -> Option<Ack> {
    if buf.len() != ACK_WIRE_LEN || buf[0] != OP_ACK {
        return None;
    }
    Some(Ack {
        seq: le_u16(buf, 1),
        kind: AckKind::from_u8(buf[3])?,
    })
}
