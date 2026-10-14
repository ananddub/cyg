//! Frame codecs for the DataChannel mesh. Encoding writes into a caller-owned
//! `Vec<u8>` so a send loop reuses one buffer; decoding borrows the datagram and
//! never allocates.

use crate::{
    MAX_ENTITIES_PER_STATE, STATE_HEADER_LEN, STATE_RECORD_LEN, dequantize_angle, dequantize_len,
    quantize_angle, quantize_len,
};

pub const OP_INPUT: u8 = 0x01;
pub const OP_STATE: u8 = 0x02;
pub const OP_EVENT: u8 = 0x03;
/// ASCII `'V'` — deliberately the same byte as `engine_voice::VOICE_MAGIC`, so a
/// voice packet is valid on the relay path and the mesh path without a rewrite.
pub const OP_VOICE: u8 = 0x56;

/// `[op][seq or tick u16]` on every non-voice frame.
pub const HEADER_LEN: usize = 3;

/// Smallest legal voice packet: the 15-byte `VoicePacket` header, no payload.
const VOICE_MIN_LEN: usize = 15;

#[inline]
fn le_u16(bytes: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([bytes[at], bytes[at + 1]])
}

#[inline]
fn le_f32(bytes: &[u8], at: usize) -> f32 {
    f32::from_le_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]])
}

#[inline]
fn put_u16(out: &mut Vec<u8>, value: u16) {
    out.extend_from_slice(&value.to_le_bytes());
}

#[inline]
fn put_i16(out: &mut Vec<u8>, value: i16) {
    out.extend_from_slice(&value.to_le_bytes());
}

#[inline]
fn put_f32(out: &mut Vec<u8>, value: f32) {
    out.extend_from_slice(&value.to_le_bytes());
}

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

    /// Events ride the ordered, retransmitting channel; everything else is
    /// replaceable and rides the unreliable one.
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

/// One player's sampled control state, sent to peers so they can predict this
/// client's motion ahead of the server's authoritative delta.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct InputFrame {
    /// Client counter, wrapping at 65536. Peers keep the highest seen and drop the rest.
    pub seq: u16,
    /// Same button mask the server's 20-byte input frame carries.
    pub flags: u32,
    pub aim_yaw: f32,
    pub aim_pitch: f32,
    pub move_x: f32,
    pub move_z: f32,
}

/// Input frame is 23 bytes: header + button mask + four floats.
pub const INPUT_WIRE_LEN: usize = HEADER_LEN + 4 + 4 * 4;

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

/// One entity in a state broadcast. `id` is the room's player id, which both ends
/// already agree on because the server stamped it into the signaling.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EntitySnapshot {
    pub id: u16,
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub yaw: f32,
    pub pitch: f32,
    /// Velocity lets the peer extrapolate across a dropped frame instead of
    /// freezing the model until the next one.
    pub vx: f32,
    pub vy: f32,
    pub vz: f32,
    /// Free-form stance/state byte (grounded, crouching, firing…).
    pub state: u8,
}

impl EntitySnapshot {
    fn write(&self, out: &mut Vec<u8>) {
        put_u16(out, self.id);
        put_i16(out, quantize_len(self.x));
        put_i16(out, quantize_len(self.y));
        put_i16(out, quantize_len(self.z));
        put_i16(out, quantize_angle(self.yaw));
        put_i16(out, quantize_angle(self.pitch));
        put_i16(out, quantize_len(self.vx));
        put_i16(out, quantize_len(self.vy));
        put_i16(out, quantize_len(self.vz));
        out.push(self.state);
    }

    fn read(bytes: &[u8]) -> Self {
        Self {
            id: le_u16(bytes, 0),
            x: dequantize_len(le_i16(bytes, 2)),
            y: dequantize_len(le_i16(bytes, 4)),
            z: dequantize_len(le_i16(bytes, 6)),
            yaw: dequantize_angle(le_i16(bytes, 8)),
            pitch: dequantize_angle(le_i16(bytes, 10)),
            vx: dequantize_len(le_i16(bytes, 12)),
            vy: dequantize_len(le_i16(bytes, 14)),
            vz: dequantize_len(le_i16(bytes, 16)),
            state: bytes[18],
        }
    }
}

#[inline]
fn le_i16(bytes: &[u8], at: usize) -> i16 {
    i16::from_le_bytes([bytes[at], bytes[at + 1]])
}

/// A borrowed state frame: the datagram stays in place, records are read on demand.
#[derive(Debug, Clone, Copy)]
pub struct StateFrame<'a> {
    pub tick: u16,
    count: u8,
    body: &'a [u8],
}

impl<'a> StateFrame<'a> {
    pub fn len(&self) -> usize {
        self.count as usize
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.count == 0
    }

    pub fn entities(&self) -> Entities<'a> {
        Entities {
            rest: self.body,
            left: self.count as usize,
        }
    }
}

#[derive(Debug)]
pub struct Entities<'a> {
    rest: &'a [u8],
    left: usize,
}

impl<'a> Iterator for Entities<'a> {
    type Item = EntitySnapshot;

    fn next(&mut self) -> Option<Self::Item> {
        if self.left == 0 {
            return None;
        }
        let (record, tail) = self.rest.split_at(STATE_RECORD_LEN);
        self.rest = tail;
        self.left -= 1;
        Some(EntitySnapshot::read(record))
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.left, Some(self.left))
    }
}

impl ExactSizeIterator for Entities<'_> {}

/// Write a state frame. Fails only when `entities` cannot fit in one datagram —
/// the caller then sends the remainder on the same tick.
pub fn encode_state(
    tick: u16,
    entities: &[EntitySnapshot],
    out: &mut Vec<u8>,
) -> Result<(), FrameError> {
    if entities.is_empty() {
        return Err(FrameError::EmptyState);
    }
    if entities.len() > MAX_ENTITIES_PER_STATE {
        return Err(FrameError::TooManyEntities {
            max: MAX_ENTITIES_PER_STATE,
            got: entities.len(),
        });
    }
    out.clear();
    out.reserve(STATE_HEADER_LEN + entities.len() * STATE_RECORD_LEN);
    out.push(OP_STATE);
    put_u16(out, tick);
    out.push(entities.len() as u8);
    for entity in entities {
        entity.write(out);
    }
    Ok(())
}

/// What happened. Small, rare, and the only frame the mesh must not lose.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum EventKind {
    Fire = 1,
    Hit = 2,
    Damage = 3,
    Kill = 4,
    Respawn = 5,
    Effect = 6,
}

impl EventKind {
    pub fn from_u8(value: u8) -> Option<Self> {
        Some(match value {
            1 => Self::Fire,
            2 => Self::Hit,
            3 => Self::Damage,
            4 => Self::Kill,
            5 => Self::Respawn,
            6 => Self::Effect,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EventFrame {
    pub seq: u16,
    pub kind: EventKind,
    pub from: u16,
    /// `u16::MAX` when the event has no target (a respawn, an area effect).
    pub target: u16,
    /// Server tick the sender was standing on, so the receiver can place the
    /// event in the same timeline the authoritative delta uses.
    pub tick: u16,
    pub value: f32,
}

pub const EVENT_WIRE_LEN: usize = HEADER_LEN + 1 + 2 + 2 + 2 + 4;

impl EventFrame {
    pub fn decode(buf: &[u8]) -> Result<Self, FrameError> {
        if buf.len() != EVENT_WIRE_LEN {
            return Err(frame_length(EVENT_WIRE_LEN, buf.len()));
        }
        Ok(Self {
            seq: le_u16(buf, 1),
            kind: EventKind::from_u8(buf[3]).ok_or(FrameError::UnknownEventKind(buf[3]))?,
            from: le_u16(buf, 4),
            target: le_u16(buf, 6),
            tick: le_u16(buf, 8),
            value: le_f32(buf, 10),
        })
    }
}

pub fn encode_event(frame: &EventFrame, out: &mut Vec<u8>) {
    out.clear();
    out.reserve(EVENT_WIRE_LEN);
    out.push(OP_EVENT);
    put_u16(out, frame.seq);
    out.push(frame.kind as u8);
    put_u16(out, frame.from);
    put_u16(out, frame.target);
    put_u16(out, frame.tick);
    put_f32(out, frame.value);
}

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
fn frame_length(want: usize, got: usize) -> FrameError {
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
