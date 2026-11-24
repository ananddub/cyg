use engine_webrtc_protocol::channel::{
    DELTA_HEADER_LEN, DeltaPacker, FragmentAssembler, MAX_DELTA_BODY, MAX_DELTA_CHUNK,
};
use proptest::prelude::*;

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545F4914F6CDD1D)
    }

    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
}

fn body_len_strategy() -> impl Strategy<Value = usize> {
    prop_oneof![
        Just(0usize),
        1usize..64,
        Just(MAX_DELTA_CHUNK),
        Just(MAX_DELTA_CHUNK - 1),
        Just(MAX_DELTA_CHUNK + 1),
        MAX_DELTA_CHUNK..(MAX_DELTA_CHUNK * 4),
        (4 * MAX_DELTA_CHUNK..=MAX_DELTA_BODY).prop_map(|l| l),
    ]
}

fn pack_frames(seq: u16, tick: u32, body: &[u8]) -> Vec<Vec<u8>> {
    let mut packer = DeltaPacker::new(seq, tick);
    let packing = packer
        .pack(body)
        .expect("body within MAX_DELTA_BODY always packs");
    let mut frames = Vec::with_capacity(packing.frag_total() as usize);
    for fragment in packing.fragments() {
        let mut wire = Vec::with_capacity(DELTA_HEADER_LEN + fragment.chunk.len());
        fragment.append_to(&mut wire);
        frames.push(wire);
    }
    frames
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(128))]

    #[test]
    fn reordered_duplicated_fragments_assemble_byte_exact(
        body_len in body_len_strategy(),
        seed in any::<u64>(),
        tick in any::<u32>(),
    ) {
        let body: Vec<u8> = (0..body_len).map(|i| (i % 251) as u8).collect();
        let mut frames = pack_frames(777, tick, &body);

        let mut rng = Rng(seed | 1);
        for i in (1..frames.len()).rev() {
            let j = rng.below(i + 1);
            frames.swap(i, j);
        }
        if !frames.is_empty() {
            let dup = rng.below(frames.len());
            frames.push(frames[dup].clone());
        }

        let mut assembler = FragmentAssembler::default();
        let mut delivered: Option<Vec<u8>> = None;
        for frame in &frames {
            if let Some(delta) = assembler.insert_frame(frame, 0) {
                assert!(delivered.is_none(), "delta delivered twice");
                delivered = Some(delta.body);
            }
        }

        let delivered = delivered.unwrap_or_else(|| {
            panic!("every fragment was delivered, so the delta must assemble")
        });
        prop_assert_eq!(delivered.len(), body.len());
        prop_assert_eq!(&delivered, &body, "assembled body must be byte-exact");
    }

    #[test]
    fn one_lost_fragment_means_never_delivered(
        body_len in body_len_strategy(),
        seed in any::<u64>(),
        tick in any::<u32>(),
    ) {
        let body: Vec<u8> = (0..body_len).map(|i| (i % 241) as u8).collect();
        let mut frames = pack_frames(888, tick, &body);
        prop_assume!(frames.len() >= 2, "a single-fragment delta cannot lose a fragment");

        let mut rng = Rng(seed | 1);
        let victim = rng.below(frames.len());
        frames.remove(victim);

        let mut assembler = FragmentAssembler::default();
        for frame in &frames {
            if let Some(delta) = assembler.insert_frame(frame, 0) {
                prop_assert!(
                    false,
                    "delta with fragment {} missing was delivered anyway ({} bytes)",
                    victim,
                    delta.body.len()
                );
            }
        }
        prop_assert_eq!(assembler.inflight(), 1, "the stalled partial stays tracked");
    }

    #[test]
    fn ttl_forgot_delta_accepts_retransmission(
        body_len in body_len_strategy(),
        _seed in any::<u64>(),
        tick in any::<u32>(),
    ) {

        let body: Vec<u8> = (0..body_len).map(|i| (i % 233) as u8).collect();
        let frames = pack_frames(999, tick, &body);
        prop_assume!(frames.len() >= 3);

        let mut assembler = FragmentAssembler::default();
        assembler.insert_frame(&frames[0], 0);
        assembler.insert_frame(&frames[1], 100);
        assembler.cleanup(100 + engine_webrtc_protocol::channel::FRAGMENT_TTL_MS + 1);

        for frame in &frames {
            if let Some(delta) = assembler.insert_frame(frame, 9_000) {
                prop_assert_eq!(&delta.body, &body, "retransmission assembles byte-exact after TTL");
                return Ok(());
            }
        }
        prop_assert!(false, "full retransmission after TTL must deliver");
    }
}
