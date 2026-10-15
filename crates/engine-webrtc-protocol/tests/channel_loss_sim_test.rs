//! The unreliable channel under load: a 5 KB delta per tick, 10 % of the fragments gone,
//! and what survives arriving in the wrong order with two or three deltas interleaved.
//!
//! The invariant is exactness, not completeness. A lost fragment means a lost tick, and no
//! amount of codec can recover it — so the loop asserts that every delta the receiver does
//! hand to the game is byte-identical to the one the sender packed, that deliveries are
//! strictly ascending in wrap-safe order, and that the receiver's buffer stays bounded and
//! drains completely once the ticks stop. Each run is repeated over several seeds, and the
//! same scheduler is run at 0 % loss to prove the reordering itself is not what corrupts.

use std::collections::{HashMap, HashSet};

use engine_webrtc_protocol::channel::{
    AssembledDelta, FRAGMENT_TTL_MS, MAX_INFLIGHT_BYTES, MAX_INFLIGHT_DELTAS, DeltaPacker,
    FragmentAssembler, is_newer,
};

const BODY_LEN: usize = 5000;
const DELTAS: usize = 30;
const LOSS_PERCENT: u64 = 10;

/// xorshift64*: reproducible from the seed alone, so a failing run names itself.
struct Rng(u64);

impl Rng {
    fn next_u64(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    fn below(&mut self, n: usize) -> usize {
        if n == 0 { 0 } else { (self.next_u64() % n as u64) as usize }
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
        .map(|k| {
            (k as u64)
                .wrapping_mul(31)
                .wrapping_add(index as u64 * 257)
                as u8
        })
        .collect()
}

/// What the receiver handed over, so a stamp can be checked against the body it came with.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Delivery {
    seq: u16,
    tick: u32,
    body: Vec<u8>,
}

impl From<AssembledDelta> for Delivery {
    fn from(delta: AssembledDelta) -> Self {
        Self { seq: delta.seq, tick: delta.tick, body: delta.body }
    }
}

struct Run {
    /// Deltas in the order the receiver delivered them.
    delivered: Vec<Delivery>,
    expected: HashMap<u16, Vec<u8>>,
    ticks: HashMap<u16, u32>,

    /// The seq of the room's last delta: the one a receiver must end up holding.
    last_seq: u16,
    lost_fragments: usize,
    assembler: FragmentAssembler,
    clock: u64,
}

/// Pack `DELTAS` ticks of 5 KB, drop `loss_percent` of their fragments, and feed the rest
/// through a scheduler that reorders them and interleaves consecutive deltas.
fn simulate(seed: u64, loss_percent: u64) -> Run {
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

        // What the channel chooses to deliver this tick, in the order it chooses.
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

    // The room keeps running after the last delta: whatever is still in flight arrives.
    rng.shuffle(&mut pending);
    for frame in pending.drain(..) {
        clock += 1;
        if let Some(delta) = assembler.insert_frame(&frame, clock) {
            delivered.push(delta.into());
        }
    }

    Run { delivered, expected, ticks, last_seq, lost_fragments, assembler, clock }
}

/// Every assertion that must hold whatever the loss rate was.
fn assert_exact(run: &Run, seed: u64, loss: u64) {
    assert!(!run.delivered.is_empty(), "seed {seed} at {loss}% loss delivered nothing");

    for delivery in &run.delivered {
        let original = run.expected.get(&delivery.seq).expect("a delivered seq was packed");
        assert_eq!(
            delivery.body, *original,
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

    // Newest wins: no seq applies twice, and no delivery goes backwards.
    let unique: HashSet<u16> = run.delivered.iter().map(|d| d.seq).collect();
    assert_eq!(unique.len(), run.delivered.len(), "a delta was delivered twice");
    for pair in run.delivered.windows(2) {
        assert!(is_newer(pair[0].seq, pair[1].seq), "deliveries went backwards: {pair:?}");
    }
    assert_eq!(
        run.assembler.newest_seq(),
        run.delivered.last().map(|d| d.seq),
        "the receiver's newest is its last delivery"
    );
}

#[test]
fn a_5kb_delta_survives_ten_percent_fragment_loss() {
    let mut any_lost = 0;
    for seed in [1u64, 42, 0x9E37_79B9_7F4A_7C15, 0xDEAD_BEEF_CAFE_1234, 0xFFFF_FFFF_0000_0001] {
        let run = simulate(seed, LOSS_PERCENT);
        assert!(run.lost_fragments > 0, "seed {seed} dropped no fragments at all");
        assert_exact(&run, seed, LOSS_PERCENT);

        // The ticks have stopped, so the stalled deltas are pure leak: TTL closes them.
        let mut run = run;
        while run.assembler.cleanup(run.clock + FRAGMENT_TTL_MS + 1) {
            run.clock += FRAGMENT_TTL_MS + 1;
        }
        assert_eq!(run.assembler.inflight(), 0, "seed {seed} pinned a half delta");
        assert_eq!(run.assembler.buffered_bytes(), 0);
        any_lost += run.lost_fragments;
    }
    assert!(any_lost > DELTAS, "the loop must actually be dropping fragments");
}

/// The same scheduler with the drops disabled: reordering and interleaving alone must
/// corrupt nothing and refuse nothing, so a failure in the lossy run is attributable to
/// loss and not to the codec.
#[test]
fn reordering_without_loss_keeps_every_delta_it_hands_over_intact() {
    for seed in [1u64, 42, 0x9E37_79B9_7F4A_7C15] {
        let run = simulate(seed, 0);
        assert_eq!(run.lost_fragments, 0);
        assert_exact(&run, seed, 0);

        // Nothing was dropped, so the room's newest delta is always recovered: it is the
        // one thing a superseded-sequence rule can never lose.
        assert_eq!(
            run.assembler.newest_seq(),
            Some(run.last_seq),
            "seed {seed}: the last tick must reach the game"
        );
        assert_eq!(
            run.delivered.last().map(|d| d.seq),
            Some(run.last_seq),
            "seed {seed}: the last delta was not the newest thing handed over"
        );
        assert_eq!(run.assembler.rejected(), 0, "the receiver refused well-formed frames");
        assert_eq!(run.assembler.duplicates(), 0, "the scheduler fed each fragment once");
    }
}

/// Determinism is part of the contract of a seeded fuzz loop: the same seed must replay
/// the same deliveries, or a reported failure cannot be reproduced.
#[test]
fn a_seed_replays_the_same_run() {
    let first = simulate(7_777, LOSS_PERCENT);
    let second = simulate(7_777, LOSS_PERCENT);
    assert_eq!(first.delivered, second.delivered);
    assert_eq!(first.lost_fragments, second.lost_fragments);
    assert_eq!(first.assembler.stale(), second.assembler.stale());
}
