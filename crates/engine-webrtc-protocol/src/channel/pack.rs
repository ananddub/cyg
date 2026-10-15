use super::delta::{DELTA_HEADER_LEN, MAX_DELTA_CHUNK};
use super::fragment::DeltaFragment;
use super::{MAX_DELTA_BODY, MAX_FRAGMENTS};
use crate::channel::DeltaHeader;

/// The one way a body can fail to be packable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PackError {
    /// Never truncate: a half-sent delta is a corrupt world state, a dropped one
    /// is last tick's.
    BodyTooLarge {
        body_len: usize,
        fragments_needed: usize,
        max_body_len: usize,
    },
}

impl std::fmt::Display for PackError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::BodyTooLarge {
                body_len,
                fragments_needed,
                max_body_len,
            } => write!(
                f,
                "delta body of {body_len} B needs {fragments_needed} fragments, \
                 over {MAX_FRAGMENTS}; the channel carries at most {max_body_len} B"
            ),
        }
    }
}

impl std::error::Error for PackError {}

/// A delta body stamped and split to the MTU, borrowing the caller's body.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeltaPacking<'a> {
    seq: u16,
    tick: u32,
    body: &'a [u8],
    total: u8,
}

impl<'a> DeltaPacking<'a> {
    /// Split `body` now, send later. Fails rather than emitting a truncated delta.
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

    /// `frag_total` for every fragment of this delta.
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

    /// Total wire bytes for all fragments — the size to reserve once per tick.
    #[inline]
    pub fn wire_len(&self) -> usize {
        DELTA_HEADER_LEN * self.total as usize + self.body.len()
    }

    /// The exact fragment list, in send order.
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

    /// Append every fragment to `out` and return how many were written, so a
    /// sender can slice them back out without a per-fragment allocation.
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

/// Iterator over a packed delta's fragments.
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
        if self.rest.is_empty() {
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

/// Pack a delta body into its fragments. `DeltaPacking` borrows the body, so a
/// high-rate loop calls this once per tick and reuses one scratch buffer to write.
pub fn pack_delta(seq: u16, tick: u32, body: &[u8]) -> Result<DeltaPacking<'_>, PackError> {
    DeltaPacking::new(seq, tick, body)
}

/// The same split as owned frames, for a caller that hands each fragment to a
/// queue rather than a socket. Allocates one `Vec` per fragment.
pub fn pack_delta_frames(seq: u16, tick: u32, body: &[u8]) -> Result<Vec<Vec<u8>>, PackError> {
    let packing = DeltaPacking::new(seq, tick, body)?;
    Ok(packing.fragments().map(|f| f.to_vec()).collect())
}
