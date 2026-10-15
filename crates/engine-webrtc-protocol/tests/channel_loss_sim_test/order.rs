use crate::harness::{LOSS_PERCENT, assert_exact, simulate};

/// The same scheduler with the drops disabled: reordering and interleaving alone
/// must corrupt nothing and refuse nothing, so a failure in the lossy run is
/// attributable to loss and not to the codec.
#[test]
fn reordering_without_loss_keeps_every_delta_it_hands_over_intact() {
    for seed in [1u64, 42, 0x9E37_79B9_7F4A_7C15] {
        let run = simulate(seed, 0);
        assert_eq!(run.lost_fragments, 0);
        assert_exact(&run, seed, 0);

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
        assert_eq!(
            run.assembler.rejected(),
            0,
            "the receiver refused well-formed frames"
        );
        assert_eq!(
            run.assembler.duplicates(),
            0,
            "the scheduler fed each fragment once"
        );
    }
}

/// Determinism is part of the contract of a seeded fuzz loop: the same seed must
/// replay the same deliveries, or a reported failure cannot be reproduced.
#[test]
fn a_seed_replays_the_same_run() {
    let first = simulate(7_777, LOSS_PERCENT);
    let second = simulate(7_777, LOSS_PERCENT);
    assert_eq!(first.delivered, second.delivered);
    assert_eq!(first.lost_fragments, second.lost_fragments);
    assert_eq!(first.assembler.stale(), second.assembler.stale());
}
