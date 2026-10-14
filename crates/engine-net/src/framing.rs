//! # Binary Framing Protocol
//!
//! Ultra-low-overhead, zero-allocation binary framing:
//! [2 bytes: payload length (u16)][1 byte: message opcode (u8)][N bytes: payload]

use bytes::{Bytes, BytesMut};

/// Standard protocol opcodes for game networking.
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
    /// Colyseus `ROOM_STATE` (14): full state snapshot for (re)joins.
    StateSnapshot = 8,
    /// Colyseus `ROOM_DATA_BYTES` (17): binary message with a type prefix.
    RoomDataBytes = 9,
    /// Colyseus `ROOM_REQUEST` (21): client request awaiting a response.
    RoomRequest = 10,
    /// Colyseus `ROOM_RESPONSE` (22): server reply to a `ROOM_REQUEST`.
    RoomResponse = 11,
    /// Colyseus `ERROR` (11): server-initiated error surfaced to `room.onError`.
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
}

/// Decodes frames from a byte buffer.
/// Returns `Some((opcode, payload))` if a complete frame is available, advancing the read index.
pub fn decode_frame(buffer: &mut Vec<u8>) -> Option<(OpCode, Vec<u8>)> {
    if buffer.len() < 3 {
        return None;
    }

    // Read 2-byte big-endian length
    let len = u16::from_be_bytes([buffer[0], buffer[1]]) as usize;
    let total_frame_len = 2 + 1 + len; // 2 bytes len + 1 byte opcode + payload len

    if buffer.len() < total_frame_len {
        return None; // Incomplete frame, wait for more bytes from socket
    }

    let opcode_byte = buffer[2];
    let opcode = OpCode::from_u8(opcode_byte).unwrap_or(OpCode::Custom);
    let payload = buffer[3..total_frame_len].to_vec();

    // Drain the consumed frame from buffer
    buffer.drain(0..total_frame_len);

    Some((opcode, payload))
}

/// Decode frames in place from a borrowed buffer, yielding zero-copy
/// payload slices. Preferred over [`decode_frame`] for datagram-style
/// reads: no `to_vec()` of the whole packet and no `drain()` memmoves.
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

/// Decode frames in place from a borrowed buffer, yielding opcodes with
/// payload byte ranges. Pair with `Bytes::slice(range)` to hand out
/// zero-copy views of a datagram held as [`Bytes`].
pub fn iter_frame_ranges(
    buf: &[u8],
) -> impl Iterator<Item = (OpCode, std::ops::Range<usize>)> {
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

/// Encodes a message into wire format and appends it to `out_buffer`.
/// Works with any [`bytes::BufMut`] sink (`Vec<u8>`, `BytesMut`, …).
pub fn encode_frame(opcode: OpCode, payload: &[u8], out_buffer: &mut impl bytes::BufMut) {
    let len = payload.len() as u16;
    out_buffer.put_slice(&len.to_be_bytes());
    out_buffer.put_u8(opcode as u8);
    out_buffer.put_slice(payload);
}

/// Pull the next complete frame off the front of a [`BytesMut`].
/// The payload is a refcounted [`Bytes`] view of the buffer — no
/// copy, and the consumed prefix is detached in O(1) (no `drain` memmove).
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
