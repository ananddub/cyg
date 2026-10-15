use crate::harness::{DELTAS, LOSS_PERCENT, assert_exact, simulate};
use engine_webrtc_protocol::channel::FRAGMENT_TTL_MS;

#[test]
fn a_5kb_delta_survives_ten_percent_fragment_loss() {
    let mut any_lost = 0;
    for seed in [
        1u64,
        42,
        0x9E37_79B9_7F4A_7C15,
        0xDEAD_BEEF_CAFE_1234,
        0xFFFF_FFFF_0000_0001,
    ] {
        let run = simulate(seed, LOSS_PERCENT);
        assert!(
            run.lost_fragments > 0,
            "seed {seed} dropped no fragments at all"
        );
        assert_exact(&run, seed, LOSS_PERCENT);

        let mut run = run;
        while run.assembler.cleanup(run.clock + FRAGMENT_TTL_MS + 1) {
            run.clock += FRAGMENT_TTL_MS + 1;
        }
        assert_eq!(
            run.assembler.inflight(),
            0,
            "seed {seed} pinned a half delta"
        );
        assert_eq!(run.assembler.buffered_bytes(), 0);
        any_lost += run.lost_fragments;
    }
    assert!(
        any_lost > DELTAS,
        "the loop must actually be dropping fragments"
    );
}
