use super::pack::{DeltaPacking, PackError};

/// The send-side half of the codec: the room-wide sequence counter plus the
/// packer that stamps it. It holds no copy of anything it has emitted, because
/// the unreliable channel's contract is to be superseded, not resent.
#[derive(Debug, Default, Clone, Copy)]
pub struct DeltaPacker {
    seq: u16,
    tick: u32,
    published: u64,
    refused: u64,
}

impl DeltaPacker {
    /// `first_seq` is what the peer's assembler has not seen yet; starting at 0
    /// is fine for a fresh room.
    pub fn new(first_seq: u16, tick: u32) -> Self {
        Self {
            seq: first_seq,
            tick,
            ..Self::default()
        }
    }

    /// The seq the next [`Self::pack`] will stamp.
    #[inline]
    pub fn next_seq(&self) -> u16 {
        self.seq
    }

    #[inline]
    pub fn tick(&self) -> u32 {
        self.tick
    }

    /// The seq is what orders deltas, the tick is what lets the client
    /// interpolate, so both follow `room.current_tick`.
    #[inline]
    pub fn set_tick(&mut self, tick: u32) {
        self.tick = tick;
    }

    /// Stamp `body` and advance the sequence, wrapping at 65536.
    pub fn pack<'b>(&mut self, body: &'b [u8]) -> Result<DeltaPacking<'b>, PackError> {
        let packing = DeltaPacking::new(self.seq, self.tick, body)?;
        self.seq = self.seq.wrapping_add(1);
        self.published += 1;
        Ok(packing)
    }

    /// Stamp and send only if the body fits, counting the delta it dropped. A
    /// refused delta does not consume a seq, so receivers never see a gap they
    /// have to explain.
    pub fn pack_or_drop<'b>(&mut self, body: &'b [u8]) -> Option<DeltaPacking<'b>> {
        match self.pack(body) {
            Ok(packing) => Some(packing),
            Err(_) => {
                self.refused += 1;
                None
            }
        }
    }

    #[inline]
    pub fn published(&self) -> u64 {
        self.published
    }

    /// Non-zero means the guest outran the channel, not the network.
    #[inline]
    pub fn refused(&self) -> u64 {
        self.refused
    }
}
