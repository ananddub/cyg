use super::delta::{DeltaHeader, MAX_DELTA_CHUNK};
use super::partial::PartialDelta;
use super::{AssembledDelta, MAX_INFLIGHT_BYTES};
use crate::channel::is_newer;
use crate::channel::is_superseded;

/// Reassembles fragmented deltas, dropping anything the game can no longer use.
///
/// Supersession is measured against the newest *delivered* seq rather than the
/// newest chunk seen, because `ordered: false` interleaves the tail of delta N
/// with the head of delta N+1 — so a bounded window of deltas is held at once,
/// whichever order their chunks arrive in.
#[derive(Debug, Default)]
pub struct FragmentAssembler {
    pub(super) newest: Option<u16>,
    /// Newest seq ever accepted, whole or not: the origin the in-flight window
    /// is measured from, so eviction can name the delta furthest behind.
    pub(super) highest: Option<u16>,
    pub(super) inflight: Vec<PartialDelta>,
    pub(super) buffered: usize,
    pub(crate) stale: u64,
    pub(crate) duplicates: u64,
    pub(crate) rejected: u64,
    pub(crate) evicted: u64,
    pub(crate) assembled: u64,
}

impl FragmentAssembler {
    /// `now_ms` is any monotonic millisecond clock the caller has; the codec
    /// crate deliberately does not own a clock.
    pub fn insert(
        &mut self,
        header: &DeltaHeader,
        chunk: &[u8],
        now_ms: u64,
    ) -> Option<AssembledDelta> {
        if chunk.len() > MAX_DELTA_CHUNK {
            self.rejected += 1;
            return None;
        }
        if header.frag_total == 0 || header.frag_idx >= header.frag_total {
            self.rejected += 1;
            return None;
        }
        if self.is_superseded(header.seq) {
            self.stale += 1;
            return None;
        }
        if self.highest.is_none_or(|h| is_newer(h, header.seq)) {
            self.highest = Some(header.seq);
        }

        if header.is_single() {
            return Some(self.deliver(header.seq, header.tick, chunk.to_vec()));
        }

        let slot = match self.inflight.iter().position(|p| p.seq == header.seq) {
            Some(pos) => pos,
            None => {
                self.make_room(chunk.len());
                if self.buffered + chunk.len() > MAX_INFLIGHT_BYTES {
                    self.rejected += 1;
                    return None;
                }
                self.inflight.push(PartialDelta {
                    seq: header.seq,
                    tick: header.tick,
                    total: header.frag_total,
                    parts: vec![None; header.frag_total as usize],
                    received: 0,
                    bytes: 0,
                    last_ms: now_ms,
                });
                self.inflight.len() - 1
            }
        };

        let conflicts = {
            let held = &self.inflight[slot];
            held.tick != header.tick || held.total != header.frag_total
        };
        if conflicts {
            self.rejected += 1;
            self.drop_slot(slot);
            return None;
        }

        let mut duplicate = false;
        let mut complete = false;
        {
            let partial = &mut self.inflight[slot];
            partial.last_ms = now_ms;
            let cell = &mut partial.parts[header.frag_idx as usize];
            if cell.is_some() {
                duplicate = true;
            } else {
                let len = chunk.len();
                *cell = Some(chunk.to_vec());
                partial.received += 1;
                partial.bytes += len;
                self.buffered += len;
                complete = partial.is_complete();
            }
        }
        if duplicate {
            self.duplicates += 1;
            return None;
        }
        if !complete {
            return None;
        }

        let partial = self.inflight.remove(slot);
        self.buffered -= partial.bytes;
        Some(self.deliver(partial.seq, partial.tick, partial.assemble()))
    }

    /// Parse a received datagram and feed it in. A frame that is not a well-formed
    /// delta is counted in [`Self::rejected`] instead of being misassembled.
    pub fn insert_frame(&mut self, frame: &[u8], now_ms: u64) -> Option<AssembledDelta> {
        match DeltaHeader::parse(frame) {
            Some((header, chunk)) => self.insert(&header, chunk, now_ms),
            None => {
                self.rejected += 1;
                None
            }
        }
    }

    /// Free of allocation and of the in-flight table, so a caller can filter
    /// before it copies anything.
    #[inline]
    pub fn is_superseded(&self, seq: u16) -> bool {
        match self.newest {
            Some(newest) => is_superseded(newest, seq),
            None => false,
        }
    }
}
