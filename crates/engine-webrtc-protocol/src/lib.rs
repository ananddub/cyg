//! # WebRTC DataChannel Game Wire Protocol
//!
//! Peer-to-peer frames for a mesh voice-and-data connection negotiated by
//! `engine_voice`'s signaling. Media-style game traffic (inputs, entity state,
//! voice) runs **directly between browsers over UDP** — the game server never
//! sees these bytes. That is the point: a retransmitted packet cannot stall the
//! ones behind it, so one loss costs one stale frame instead of a freeze.
//!
//! Server authority is untouched. Inputs still reach the room over the
//! WebSocket/TCP path, and the server still decides damage. What the mesh gives
//! you is remote entities and voice at roughly one hop fewer than the relay path
//! would cost, plus a reliable side channel for things that must not be dropped.
//!
//! ## Wire layout
//!
//! Everything is little-endian (the browser `DataView` default) and the first
//! byte is the opcode, so a `Uint8Array` can be dispatched with one compare:
//!
//! | Opcode | Frame    | Channel   | Layout |
//! |--------|----------|-----------|--------|
//! | `0x01` | Input    | unreliable| `[op][seq u16][flags u32][aimYaw f32][aimPitch f32][moveX f32][moveZ f32]` (23 B) |
//! | `0x02` | State    | unreliable| `[op][tick u16][count u8]` + `count` × 19 B records |
//! | `0x03` | Event    | reliable  | `[op][seq u16][kind u8][from u16][target u16][tick u16][value f32]` (14 B) |
//! | `0x56` | Voice    | unreliable| the `engine_voice::VoicePacket` **verbatim** |
//!
//! A state record is 19 bytes:
//! `[id u16][x i16][y i16][z i16][yaw i16][pitch i16][vx i16][vy i16][vz i16][state u8]`
//!
//! ## Why voice is `0x56` and not a new opcode
//!
//! `0x56` is ASCII `'V'`, which is already the first byte of a `VoicePacket` on
//! the relay path (engine-voice calls it `VOICE_MAGIC`). Reusing it means the
//! *same* buffer a client hands to `sendVoice()` can be written straight to a
//! DataChannel with no header rewrite and no copy — and the `'V'`/`'W'` sniffing
//! the server already does keeps working unchanged on both ends.
//!
//! ## Channels
//!
//! Two DataChannels per peer, both on the one `RTCPeerConnection` that carries the
//! voice track:
//!
//! - `game-unreliable` — `{ ordered: false, maxRetransmits: 0 }` for input, state
//!   and voice. A stale frame is worthless, so SCTP is told not to resend.
//! - `game-reliable`   — `{ ordered: true }` for events. Shots, hits and respawns
//!   may arrive late but never vanish.
//!
//! ## Sizing
//!
//! Frames are built to fit one packet: [`MAX_FRAME`] is 1200 bytes, under WebRTC's
//! ~1200 B path-MTU target, so a frame is never fragmented by SCTP. A state frame
//! therefore carries at most [`MAX_ENTITIES_PER_STATE`] records; the sender
//! batches the rest into the next frame on the same tick.
//!
//! ## Quantization
//!
//! Positions and speeds use [`LEN_SCALE`] (1/16 of a world unit, saturating at
//! ±2047.9), which is 6.25 cm at tank scale and well below the server's own
//! collision epsilon. Angles are quantised over a full turn, giving ~0.0055°.
//! Both are saturating rather than fallible: a wild float costs precision on one
//! frame, not a dropped connection.

pub mod frames;

/// Positions and velocities are stored as `value * 16` in an `i16`.
pub const LEN_SCALE: f32 = 16.0;

/// Angles are stored as a fraction of a full turn in an `i16`.
pub const ANGLE_SCALE: f32 = 32768.0;

/// Keep a frame inside one SCTP/UDP packet.
pub const MAX_FRAME: usize = 1200;

/// Bytes per entity in a state frame.
pub const STATE_RECORD_LEN: usize = 19;

/// Header of a state frame: `[op][tick u16][count u8]`.
pub const STATE_HEADER_LEN: usize = 4;

/// Most records that fit in one state frame.
pub const MAX_ENTITIES_PER_STATE: usize = (MAX_FRAME - STATE_HEADER_LEN) / STATE_RECORD_LEN;

/// Round-to-nearest, saturating fixed-point length. Non-finite input becomes the
/// saturating extreme (`f32 as i16` in Rust and `Math.round` + clamp in JS agree).
#[inline]
pub fn quantize_len(value: f32) -> i16 {
    let scaled = (value * LEN_SCALE).round();
    scaled.clamp(i16::MIN as f32, i16::MAX as f32) as i16
}

#[inline]
pub fn dequantize_len(raw: i16) -> f32 {
    raw as f32 / LEN_SCALE
}

/// Wrap to `[-pi, pi)` then quantise over a full turn.
#[inline]
pub fn quantize_angle(value: f32) -> i16 {
    const TAU: f32 = std::f32::consts::TAU;
    let mut a = value % TAU;
    if a > core::f32::consts::PI {
        a -= TAU;
    } else if a < -core::f32::consts::PI {
        a += TAU;
    }
    let scaled = (a / TAU * 2.0 * ANGLE_SCALE).round();
    scaled.clamp(i16::MIN as f32, i16::MAX as f32) as i16
}

#[inline]
pub fn dequantize_angle(raw: i16) -> f32 {
    raw as f32 * (std::f32::consts::TAU / (2.0 * ANGLE_SCALE))
}

pub use frames::{
    EVENT_WIRE_LEN, Entities, EntitySnapshot, EventFrame, EventKind, FrameError, HEADER_LEN,
    INPUT_WIRE_LEN, InputFrame, MeshFrame, OP_EVENT, OP_INPUT, OP_STATE, OP_VOICE, Opcode,
    StateFrame, decode, encode_event, encode_input, encode_state, header, is_newer,
};
