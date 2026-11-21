use bytes::{Bytes, BytesMut};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum OpCode {
    Ping = 0,
    Pong = 1,
    JoinRoom = 2,
    LeaveRoom = 3,
    PlayerInput = 4,
    StateDelta = 5,
    Custom = 6,
    RewindHitCheck = 7,

    StateSnapshot = 8,

    RoomDataBytes = 9,

    RoomRequest = 10,

    RoomResponse = 11,

    Error = 12,
}

impl OpCode {
    pub fn from_u8(val: u8) -> Option<Self> {
        match val {
            0 => Some(Self::Ping),
            1 => Some(Self::Pong),
            2 => Some(Self::JoinRoom),
            3 => Some(Self::LeaveRoom),
            4 => Some(Self::PlayerInput),
            5 => Some(Self::StateDelta),
            6 => Some(Self::Custom),
            7 => Some(Self::RewindHitCheck),
            8 => Some(Self::StateSnapshot),
            9 => Some(Self::RoomDataBytes),
            10 => Some(Self::RoomRequest),
            11 => Some(Self::RoomResponse),
            12 => Some(Self::Error),
            _ => None,
        }
    }

    pub fn from_wire(code: u8) -> Option<Self> {
        match code {
            13 => Some(Self::PlayerInput),
            14 => Some(Self::StateSnapshot),
            15 => Some(Self::StateDelta),
            17 => Some(Self::RoomDataBytes),
            _ => None,
        }
    }
}

pub fn decode_frame(buffer: &mut Vec<u8>) -> Option<(OpCode, Vec<u8>)> {
    if buffer.len() < 3 {
        return None;
    }

    let len = u16::from_be_bytes([buffer[0], buffer[1]]) as usize;
    let total_frame_len = 2 + 1 + len;

    if buffer.len() < total_frame_len {
        return None;
    }

    let opcode_byte = buffer[2];
    let opcode = OpCode::from_u8(opcode_byte).unwrap_or(OpCode::Custom);
    let payload = buffer[3..total_frame_len].to_vec();

    buffer.drain(0..total_frame_len);

    Some((opcode, payload))
}

pub fn iter_frames(buf: &[u8]) -> impl Iterator<Item = (OpCode, &[u8])> {
    let mut pos = 0usize;
    std::iter::from_fn(move || {
        if buf.len() < pos + 3 {
            return None;
        }
        let len = u16::from_be_bytes([buf[pos], buf[pos + 1]]) as usize;
        let total = 2 + 1 + len;
        if buf.len() < pos + total {
            return None;
        }
        let opcode = OpCode::from_u8(buf[pos + 2]).unwrap_or(OpCode::Custom);
        let payload = &buf[pos + 3..pos + total];
        pos += total;
        Some((opcode, payload))
    })
}

pub fn iter_frame_ranges(buf: &[u8]) -> impl Iterator<Item = (OpCode, std::ops::Range<usize>)> {
    let mut pos = 0usize;
    std::iter::from_fn(move || {
        if buf.len() < pos + 3 {
            return None;
        }
        let len = u16::from_be_bytes([buf[pos], buf[pos + 1]]) as usize;
        let total = 2 + 1 + len;
        if buf.len() < pos + total {
            return None;
        }
        let opcode = OpCode::from_u8(buf[pos + 2]).unwrap_or(OpCode::Custom);
        let range = pos + 3..pos + total;
        pos += total;
        Some((opcode, range))
    })
}

/// Encode one length-prefixed frame. Errors when the payload exceeds the
/// 16-bit length prefix (`u16::MAX`) — nothing is written on error, so the
/// caller can drop the frame instead of silently corrupting the stream.
pub fn encode_frame(
    opcode: OpCode,
    payload: &[u8],
    out_buffer: &mut impl bytes::BufMut,
) -> std::io::Result<()> {
    if payload.len() > u16::MAX as usize {
        debug_assert!(false, "frame payload of {} bytes exceeds u16::MAX", payload.len());
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("frame payload of {} bytes exceeds the u16 length prefix", payload.len()),
        ));
    }
    let len = payload.len() as u16;
    out_buffer.put_slice(&len.to_be_bytes());
    out_buffer.put_u8(opcode as u8);
    out_buffer.put_slice(payload);
    Ok(())
}

pub fn take_frame(buf: &mut BytesMut) -> Option<(OpCode, Bytes)> {
    if buf.len() < 3 {
        return None;
    }
    let len = u16::from_be_bytes([buf[0], buf[1]]) as usize;
    let total = 2 + 1 + len;
    if buf.len() < total {
        return None;
    }
    let frame = buf.split_to(total).freeze();
    let opcode = OpCode::from_u8(frame[2]).unwrap_or(OpCode::Custom);
    Some((opcode, frame.slice(3..total)))
}
