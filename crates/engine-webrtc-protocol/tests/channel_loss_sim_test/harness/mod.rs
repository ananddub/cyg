use std::collections::HashMap;

use engine_webrtc_protocol::channel::{AssembledDelta, FragmentAssembler};

mod generate;
mod verify;

pub(crate) use generate::simulate;
pub(crate) use verify::assert_exact;

pub(crate) const BODY_LEN: usize = 5000;
pub(crate) const DELTAS: usize = 30;
pub(crate) const LOSS_PERCENT: u64 = 10;

pub(crate) struct Rng(pub u64);

impl Rng {
    fn next_u64(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    fn below(&mut self, n: usize) -> usize {
        if n == 0 {
            0
        } else {
            (self.next_u64() % n as u64) as usize
        }
    }

    fn loses(&mut self, percent: u64) -> bool {
        self.next_u64() % 100 < percent
    }

    fn shuffle<T>(&mut self, items: &mut [T]) {
        for i in (1..items.len()).rev() {
            items.swap(i, self.below(i + 1));
        }
    }
}

fn body_for(index: usize) -> Vec<u8> {
    (0..BODY_LEN)
        .map(|k| (k as u64).wrapping_mul(31).wrapping_add(index as u64 * 257) as u8)
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Delivery {
    pub seq: u16,
    pub tick: u32,
    pub body: Vec<u8>,
}

impl From<AssembledDelta> for Delivery {
    fn from(delta: AssembledDelta) -> Self {
        Self {
            seq: delta.seq,
            tick: delta.tick,
            body: delta.body,
        }
    }
}

pub(crate) struct Run {
    pub delivered: Vec<Delivery>,
    expected: HashMap<u16, Vec<u8>>,
    ticks: HashMap<u16, u32>,
    pub last_seq: u16,
    pub lost_fragments: usize,
    pub assembler: FragmentAssembler,
    pub clock: u64,
}
