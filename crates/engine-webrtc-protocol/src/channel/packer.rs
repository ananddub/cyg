use super::pack::{DeltaPacking, PackError};

#[derive(Debug, Default, Clone, Copy)]
pub struct DeltaPacker {
    seq: u16,
    tick: u32,
    published: u64,
    refused: u64,
}

impl DeltaPacker {

    pub fn new(first_seq: u16, tick: u32) -> Self {
        Self {
            seq: first_seq,
            tick,
            ..Self::default()
        }
    }

    #[inline]
    pub fn next_seq(&self) -> u16 {
        self.seq
    }

    #[inline]
    pub fn tick(&self) -> u32 {
        self.tick
    }

    #[inline]
    pub fn set_tick(&mut self, tick: u32) {
        self.tick = tick;
    }

    pub fn pack<'b>(&mut self, body: &'b [u8]) -> Result<DeltaPacking<'b>, PackError> {
        let packing = DeltaPacking::new(self.seq, self.tick, body)?;
        self.seq = self.seq.wrapping_add(1);
        self.published += 1;
        Ok(packing)
    }

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

    #[inline]
    pub fn refused(&self) -> u64 {
        self.refused
    }
}
