use super::{FrameError, HEADER_LEN, frame_length};
use crate::bytes::{le_f32, le_u16, put_f32, put_u16};

pub const OP_INPUT: u8 = 0x01;

/// Input frame is 23 bytes: header + button mask + four floats.
pub const INPUT_WIRE_LEN: usize = HEADER_LEN + 4 + 4 * 4;

/// One player's sampled control state, sent to peers so they can predict this
/// client's motion ahead of the server's authoritative delta.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct InputFrame {
    pub seq: u16,
    pub flags: u32,
    pub aim_yaw: f32,
    pub aim_pitch: f32,
    pub move_x: f32,
    pub move_z: f32,
}

impl InputFrame {
    pub fn decode(buf: &[u8]) -> Result<Self, FrameError> {
        if buf.len() != INPUT_WIRE_LEN {
            return Err(frame_length(INPUT_WIRE_LEN, buf.len()));
        }
        Ok(Self {
            seq: le_u16(buf, 1),
            flags: u32::from_le_bytes([buf[3], buf[4], buf[5], buf[6]]),
            aim_yaw: le_f32(buf, 7),
            aim_pitch: le_f32(buf, 11),
            move_x: le_f32(buf, 15),
            move_z: le_f32(buf, 19),
        })
    }
}

/// Write an input frame into `out`, starting a fresh frame.
pub fn encode_input(frame: &InputFrame, out: &mut Vec<u8>) {
    out.clear();
    out.reserve(INPUT_WIRE_LEN);
    out.push(OP_INPUT);
    put_u16(out, frame.seq);
    out.extend_from_slice(&frame.flags.to_le_bytes());
    put_f32(out, frame.aim_yaw);
    put_f32(out, frame.aim_pitch);
    put_f32(out, frame.move_x);
    put_f32(out, frame.move_z);
}
