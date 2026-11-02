mod error;
mod event;
mod input;
mod state;

pub use error::FrameError;
pub use event::{EVENT_WIRE_LEN, EventFrame, EventKind, OP_EVENT, encode_event};
pub use input::{INPUT_WIRE_LEN, InputFrame, OP_INPUT, encode_input};
pub use state::{Entities, EntitySnapshot, OP_STATE, StateFrame, encode_state};

use crate::MAX_ENTITIES_PER_STATE;
use crate::MAX_FRAME;
use crate::STATE_HEADER_LEN;
use crate::STATE_RECORD_LEN;
use crate::bytes::le_u16;

pub const OP_VOICE: u8 = 0x56;

pub const HEADER_LEN: usize = 3;

const VOICE_MIN_LEN: usize = 15;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Opcode {
    Input = OP_INPUT,
    State = OP_STATE,
    Event = OP_EVENT,
    Voice = OP_VOICE,
}

impl Opcode {
    pub fn from_u8(value: u8) -> Option<Self> {
        Some(match value {
            OP_INPUT => Self::Input,
            OP_STATE => Self::State,
            OP_EVENT => Self::Event,
            OP_VOICE => Self::Voice,
            _ => return None,
        })
    }

    #[inline]
    pub fn is_reliable(self) -> bool {
        matches!(self, Self::Event)
    }
}

#[derive(Debug, Clone, Copy)]
pub enum MeshFrame<'a> {
    Input(InputFrame),
    State(StateFrame<'a>),
    Event(EventFrame),
    Voice(&'a [u8]),
}

#[inline]
pub(crate) fn frame_length(want: usize, got: usize) -> FrameError {
    FrameError::LengthMismatch { want, got }
}

pub fn header(buf: &[u8]) -> Option<(Opcode, u16)> {
    let (op, rest) = buf.split_first().map(|(a, b)| (*a, b))?;
    let opcode = Opcode::from_u8(op)?;
    if opcode == Opcode::Voice {
        return Some((opcode, 0));
    }
    if rest.len() < 2 {
        return None;
    }
    Some((opcode, le_u16(buf, 1)))
}

pub fn decode(buf: &[u8]) -> Result<MeshFrame<'_>, FrameError> {
    let (&op, _) = buf
        .split_first()
        .ok_or(FrameError::TooShort { need: 1, got: 0 })?;
    match Opcode::from_u8(op).ok_or(FrameError::UnknownOpcode(op))? {
        Opcode::Input => Ok(MeshFrame::Input(InputFrame::decode(buf)?)),
        Opcode::State => {
            if buf.len() < STATE_HEADER_LEN {
                return Err(FrameError::TooShort {
                    need: STATE_HEADER_LEN,
                    got: buf.len(),
                });
            }
            let count = buf[3] as usize;
            if count == 0 {
                return Err(FrameError::EmptyState);
            }
            if count > MAX_ENTITIES_PER_STATE {
                return Err(FrameError::TooManyEntities {
                    max: MAX_ENTITIES_PER_STATE,
                    got: count,
                });
            }
            if buf.len() > MAX_FRAME {
                return Err(FrameError::LengthMismatch {
                    want: MAX_FRAME,
                    got: buf.len(),
                });
            }
            let want = STATE_HEADER_LEN + count * STATE_RECORD_LEN;
            if buf.len() != want {
                return Err(frame_length(want, buf.len()));
            }
            Ok(MeshFrame::State(StateFrame {
                tick: le_u16(buf, 1),
                count: buf[3],
                body: &buf[STATE_HEADER_LEN..],
            }))
        }
        Opcode::Event => Ok(MeshFrame::Event(EventFrame::decode(buf)?)),
        Opcode::Voice => {
            if buf.len() < VOICE_MIN_LEN {
                return Err(FrameError::TooShort {
                    need: VOICE_MIN_LEN,
                    got: buf.len(),
                });
            }
            Ok(MeshFrame::Voice(buf))
        }
    }
}

#[inline]
pub fn is_newer(last: u16, tick: u16) -> bool {
    let delta = tick.wrapping_sub(last);
    delta != 0 && delta < 0x8000
}
