use std::collections::HashSet;

use engine_webrtc_protocol::channel::is_newer;

use super::Run;

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
    assert_eq!(unique.len(), run.delivered.len(), "a delta was delivered twice");
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
