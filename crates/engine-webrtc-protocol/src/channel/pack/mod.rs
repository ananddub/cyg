use super::MAX_DELTA_BODY;
use super::delta::{DELTA_HEADER_LEN, MAX_DELTA_CHUNK};
use super::fragment::DeltaFragment;
use crate::channel::DeltaHeader;

mod error;

pub use error::PackError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeltaPacking<'a> {
    seq: u16,
    tick: u32,
    body: &'a [u8],
    total: u8,
}

impl<'a> DeltaPacking<'a> {
    pub fn new(seq: u16, tick: u32, body: &'a [u8]) -> Result<Self, PackError> {
        let Some(total) = super::delta_fragment_count(body.len()) else {
            return Err(PackError::BodyTooLarge {
                body_len: body.len(),
                fragments_needed: body.len().div_ceil(MAX_DELTA_CHUNK),
                max_body_len: MAX_DELTA_BODY,
            });
        };
        Ok(Self {
            seq,
            tick,
            body,
            total,
        })
    }

    #[inline]
    pub fn seq(&self) -> u16 {
        self.seq
    }

    #[inline]
    pub fn tick(&self) -> u32 {
        self.tick
    }

    #[inline]
    pub fn frag_total(&self) -> u8 {
        self.total
    }

    #[inline]
    pub fn body(&self) -> &'a [u8] {
        self.body
    }

    #[inline]
    pub fn body_len(&self) -> usize {
        self.body.len()
    }

    #[inline]
    pub fn wire_len(&self) -> usize {
        DELTA_HEADER_LEN * self.total as usize + self.body.len()
    }

    #[inline]
    pub fn fragments(&self) -> Fragments<'a> {
        Fragments {
            seq: self.seq,
            tick: self.tick,
            total: self.total,
            rest: self.body,
            idx: 0,
        }
    }

    pub fn append_frames_to(&self, out: &mut Vec<u8>) -> usize {
        let before = out.len();
        out.reserve(self.wire_len());
        for fragment in self.fragments() {
            fragment.append_to(out);
        }
        debug_assert_eq!(out.len() - before, self.wire_len());
        self.total as usize
    }
}

#[derive(Debug, Clone)]
pub struct Fragments<'a> {
    seq: u16,
    tick: u32,
    total: u8,
    rest: &'a [u8],
    idx: u8,
}

impl<'a> Iterator for Fragments<'a> {
    type Item = DeltaFragment<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.idx >= self.total {
            return None;
        }
        let chunk = &self.rest[..self.rest.len().min(MAX_DELTA_CHUNK)];
        self.rest = &self.rest[chunk.len()..];
        let fragment = DeltaFragment {
            header: DeltaHeader {
                seq: self.seq,
                tick: self.tick,
                frag_idx: self.idx,
                frag_total: self.total,
            },
            chunk,
        };
        self.idx += 1;
        Some(fragment)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let left = (self.total - self.idx) as usize;
        (left, Some(left))
    }
}

impl ExactSizeIterator for Fragments<'_> {}

pub fn pack_delta(seq: u16, tick: u32, body: &[u8]) -> Result<DeltaPacking<'_>, PackError> {
    DeltaPacking::new(seq, tick, body)
}

pub fn pack_delta_frames(seq: u16, tick: u32, body: &[u8]) -> Result<Vec<Vec<u8>>, PackError> {
    let packing = DeltaPacking::new(seq, tick, body)?;
    Ok(packing.fragments().map(|f| f.to_vec()).collect())
}
