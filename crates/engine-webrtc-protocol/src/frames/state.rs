use super::FrameError;
use crate::STATE_RECORD_LEN;
use crate::bytes::{le_i16, le_u16, put_i16, put_u16};
use crate::dequantize_angle;
use crate::dequantize_len;
use crate::quantize_angle;
use crate::quantize_len;
use crate::{MAX_ENTITIES_PER_STATE, STATE_HEADER_LEN};

pub const OP_STATE: u8 = 0x02;

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
    pub vx: f32,
    pub vy: f32,
    pub vz: f32,
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

/// A borrowed state frame: the datagram stays in place, records are read on demand.
#[derive(Debug, Clone, Copy)]
pub struct StateFrame<'a> {
    pub tick: u16,
    pub(super) count: u8,
    pub(super) body: &'a [u8],
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
