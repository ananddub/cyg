//! # Binary Framing Protocol
//!
//! Ultra-low-overhead, zero-allocation binary framing:
//! [2 bytes: payload length (u16)][1 byte: message opcode (u8)][N bytes: payload]

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

/// Encodes a message into wire format and appends it to `out_buffer`.
pub fn encode_frame(opcode: OpCode, payload: &[u8], out_buffer: &mut Vec<u8>) {
    let len = payload.len() as u16;
    out_buffer.extend_from_slice(&len.to_be_bytes());
    out_buffer.push(opcode as u8);
    out_buffer.extend_from_slice(payload);
}
