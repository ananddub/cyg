use std::collections::{HashMap, HashSet};

use engine_webrtc_protocol::channel::{
    AssembledDelta, DeltaPacker, FragmentAssembler, MAX_INFLIGHT_BYTES, MAX_INFLIGHT_DELTAS,
    is_newer,
};

pub(crate) const BODY_LEN: usize = 5000;
pub(crate) const DELTAS: usize = 30;
pub(crate) const LOSS_PERCENT: u64 = 10;

/// xorshift64*: reproducible from the seed alone, so a failing run names itself.
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

/// Distinct per delta, so a body that leaked from one seq to another cannot pass as equal.
fn body_for(index: usize) -> Vec<u8> {
    (0..BODY_LEN)
        .map(|k| (k as u64).wrapping_mul(31).wrapping_add(index as u64 * 257) as u8)
        .collect()
}

/// What the receiver handed over, so a stamp can be checked against the body it came with.
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
    /// Deltas in the order the receiver delivered them.
    pub delivered: Vec<Delivery>,
    expected: HashMap<u16, Vec<u8>>,
    ticks: HashMap<u16, u32>,
    /// The seq of the room's last delta: the one a receiver must end up holding.
    pub last_seq: u16,
    pub lost_fragments: usize,
    pub assembler: FragmentAssembler,
    pub clock: u64,
}

/// Pack `DELTAS` ticks of 5 KB, drop `loss_percent` of their fragments, and feed
/// the rest through a scheduler that reorders them and interleaves consecutive
/// deltas.
pub(crate) fn simulate(seed: u64, loss_percent: u64) -> Run {
    let mut rng = Rng(seed);
    let mut assembler = FragmentAssembler::default();
    let mut packer = DeltaPacker::new(u16::MAX - (DELTAS as u16 / 2), 1_000);
    let mut expected = HashMap::new();
    let mut ticks = HashMap::new();
    let mut delivered = Vec::new();
    let mut lost_fragments = 0;
    let mut clock = 0u64;
    let mut pending: Vec<Vec<u8>> = Vec::new();
    let mut last_seq = 0;

    for index in 0..DELTAS {
        let body = body_for(index);
        let seq = packer.next_seq();
        let packing = packer.pack(&body).expect("5 KB packs");
        assert_eq!(packing.seq(), seq);
        assert_eq!(packing.frag_total(), 5);
        ticks.insert(seq, packing.tick());
        last_seq = seq;

        let mut frames: Vec<Vec<u8>> = packing.fragments().map(|f| f.to_vec()).collect();
        expected.insert(seq, body);
        rng.shuffle(&mut frames);
        for frame in frames {
            if rng.loses(loss_percent) {
                lost_fragments += 1;
            } else {
                pending.push(frame);
            }
        }

        rng.shuffle(&mut pending);
        let take = rng.below(pending.len().min(4)) + 1;
        let batch: Vec<Vec<u8>> = pending.drain(..take).collect();
        for frame in batch {
            clock += 1;
            if let Some(delta) = assembler.insert_frame(&frame, clock) {
                delivered.push(delta.into());
            }
        }
        assert!(assembler.inflight() <= MAX_INFLIGHT_DELTAS);
        assert!(assembler.buffered_bytes() <= MAX_INFLIGHT_BYTES);
    }

    rng.shuffle(&mut pending);
    for frame in pending.drain(..) {
        clock += 1;
        if let Some(delta) = assembler.insert_frame(&frame, clock) {
            delivered.push(delta.into());
        }
    }

    Run {
        delivered,
        expected,
        ticks,
        last_seq,
        lost_fragments,
        assembler,
        clock,
    }
}

/// Every assertion that must hold whatever the loss rate was.
pub(crate) fn assert_exact(run: &Run, seed: u64, loss: u64) {
    assert!(
        !run.delivered.is_empty(),
        "seed {seed} at {loss}% loss delivered nothing"
    );

    for delivery in &run.delivered {
        let original = run
            .expected
            .get(&delivery.seq)
            .expect("a delivered seq was packed");
        assert_eq!(
            delivery.body,
            *original,
            "seed {seed} at {loss}%: delta {} reassembled to {}/{} matching bytes",
            delivery.seq,
            delivery
                .body
                .iter()
                .zip(original.iter())
                .filter(|(a, b)| a == b)
                .count(),
            original.len(),
        );
        assert_eq!(
            Some(delivery.tick),
            run.ticks.get(&delivery.seq).copied(),
            "seed {seed}: delta {} arrived stamped with another tick",
            delivery.seq
        );
    }

    let unique: HashSet<u16> = run.delivered.iter().map(|d| d.seq).collect();
    assert_eq!(
        unique.len(),
        run.delivered.len(),
        "a delta was delivered twice"
    );
    for pair in run.delivered.windows(2) {
        assert!(
            is_newer(pair[0].seq, pair[1].seq),
            "deliveries went backwards: {pair:?}"
        );
    }
    assert_eq!(
        run.assembler.newest_seq(),
        run.delivered.last().map(|d| d.seq),
        "the receiver's newest is its last delivery"
    );
}
