pub mod channel;
pub mod frames;

mod bytes;

pub const LEN_SCALE: f32 = 16.0;

pub const ANGLE_SCALE: f32 = 32768.0;

pub const MAX_FRAME: usize = 1200;

pub const STATE_RECORD_LEN: usize = 19;

pub const STATE_HEADER_LEN: usize = 4;

pub const MAX_ENTITIES_PER_STATE: usize = (MAX_FRAME - STATE_HEADER_LEN) / STATE_RECORD_LEN;

#[inline]
pub fn quantize_len(value: f32) -> i16 {
    let scaled = (value * LEN_SCALE).round();
    scaled.clamp(i16::MIN as f32, i16::MAX as f32) as i16
}

#[inline]
pub fn dequantize_len(raw: i16) -> f32 {
    raw as f32 / LEN_SCALE
}

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

pub use channel::{
    ACK_WIRE_LEN, Ack, AckKind, AssembledDelta, CHECKSUM_WIRE_LEN, ChecksumReport,
    DELTA_HEADER_LEN, DeltaHeader, FRAGMENT_TTL_MS, FragmentAssembler, MAX_DELTA_BODY,
    MAX_DELTA_CHUNK, MAX_FRAGMENTS, OP_ACK, OP_CHECKSUM, OP_DELTA, delta_fragment_count, parse_ack,
    parse_checksum, write_ack, write_checksum, write_delta_frame,
};
pub use frames::{
    EVENT_WIRE_LEN, Entities, EntitySnapshot, EventFrame, EventKind, FrameError, HEADER_LEN,
    INPUT_WIRE_LEN, InputFrame, MeshFrame, OP_EVENT, OP_INPUT, OP_STATE, OP_VOICE, Opcode,
    StateFrame, decode, encode_event, encode_input, encode_state, header, is_newer,
};
