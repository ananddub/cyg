mod event;
mod input;
mod state;

pub use event::{EVENT_WIRE_LEN, EventFrame, EventKind, OP_EVENT, encode_event};
pub use input::{INPUT_WIRE_LEN, InputFrame, OP_INPUT, encode_input};
pub use state::{Entities, EntitySnapshot, OP_STATE, StateFrame, encode_state};

use crate::STATE_HEADER_LEN;
use crate::STATE_RECORD_LEN;
use crate::bytes::le_u16;

/// ASCII `'V'` — the same first byte as `engine_voice::VOICE_MAGIC`, so one voice
/// buffer is valid on the relay path and the mesh path without a rewrite.
pub const OP_VOICE: u8 = 0x56;

/// `[op][seq or tick u16]` on every non-voice frame.
pub const HEADER_LEN: usize = 3;

/// Smallest legal voice packet: the 15-byte `VoicePacket` header, no payload.
const VOICE_MIN_LEN: usize = 15;

/// Which mesh frame this byte stream holds.
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

    /// Events ride the ordered, retransmitting channel; everything else is replaceable.
    #[inline]
    pub fn is_reliable(self) -> bool {
        matches!(self, Self::Event)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameError {
    TooShort {
        need: usize,
        got: usize,
    },
    UnknownOpcode(u8),
    UnknownEventKind(u8),
    /// A datagram holds exactly one frame; trailing bytes mean a miscount upstream.
    LengthMismatch {
        want: usize,
        got: usize,
    },
    TooManyEntities {
        max: usize,
        got: usize,
    },
    EmptyState,
}

impl std::fmt::Display for FrameError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TooShort { need, got } => write!(f, "frame needs {need} bytes, got {got}"),
            Self::UnknownOpcode(v) => write!(f, "unassigned opcode 0x{v:02X}"),
            Self::UnknownEventKind(v) => write!(f, "unassigned event kind {v}"),
            Self::LengthMismatch { want, got } => {
                write!(f, "frame should be {want} bytes, got {got}")
            }
            Self::TooManyEntities { max, got } => {
                write!(f, "{got} entities in one state frame, max {max}")
            }
            Self::EmptyState => write!(f, "state frame with no entities"),
        }
    }
}

impl std::error::Error for FrameError {}

/// A decoded mesh frame. Voice borrows the whole datagram, including its `'V'`
/// opcode, so `engine_voice::VoicePacket::decode` accepts it unchanged.
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

/// `[opcode, seq-or-tick]` without decoding the body — enough to pick a channel
/// or drop a stale frame before paying for a full parse.
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

/// Parse one datagram. Anything that does not match a frame exactly is an error,
/// so a corrupt peer cannot make the receiver read past its own buffer.
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

/// True when `tick` should replace `last`, using wrap-around sequence comparison.
/// Shared by both ends so "newest wins" means the same thing in Rust and JS.
#[inline]
pub fn is_newer(last: u16, tick: u16) -> bool {
    let delta = tick.wrapping_sub(last);
    delta != 0 && delta < 0x8000
}
