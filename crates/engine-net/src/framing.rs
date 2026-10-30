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
    /// Full state snapshot for (re)joins.
    StateSnapshot = 8,
    /// Opaque binary room payload. Internal tag only — the wire header code is
    /// `17`, applied by `transport::ws::protocol::wire_opcode_for`.
    RoomDataBytes = 9,
    /// Client request awaiting a response (wire header code `21`).
    RoomRequest = 10,
    /// Server reply to a [`OpCode::RoomRequest`] (wire header code `22`).
    RoomResponse = 11,
    /// Server-initiated error surfaced to `room.onError` (wire header code `11`).
    Error = 12,
}

impl OpCode {
    /// Inverse of `opcode as u8` for the *internal* tag space the generic
    /// framing helpers use.
    ///
    /// This is **not** the wire protocol. The client-facing header codes differ
    /// (`10` = JOIN_ROOM, `11` = ERROR, `12` = LEAVE_ROOM, `15` = STATE_PATCH,
    /// `17` = ROOM_DATA_BYTES, `18` = PING, `21` = ROOM_REQUEST, `22` =
    /// ROOM_RESPONSE); the WebSocket transport maps between the two in
    /// `transport::ws::protocol`, and `engine-server::netcode` owns the game
    /// layer's canonical opcodes.
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

    /// Map a client-facing wire-protocol header code onto the engine tag.
    ///
    /// Mirrors the inbound table in `transport::ws::frames` and the inverse of
    /// `transport::ws::protocol::wire_opcode_for`. Used by the game layer to put
    /// a relayed packet back under the header its peers expect. Handshake and
    /// control codes (`10`, `11`, `12`, `18`, `21`, `22`) have no game-layer
    /// meaning here and return `None`.
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
