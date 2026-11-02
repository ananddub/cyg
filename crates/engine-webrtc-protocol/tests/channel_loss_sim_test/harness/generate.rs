use std::collections::HashMap;

use engine_webrtc_protocol::channel::{
    DeltaPacker, FragmentAssembler, MAX_INFLIGHT_BYTES, MAX_INFLIGHT_DELTAS,
};

use super::{DELTAS, Rng, Run, body_for};

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
