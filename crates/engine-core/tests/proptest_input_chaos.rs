use engine_core::{PlayerInput, PlayerInputBuffer};
use proptest::prelude::*;

#[derive(Debug, Clone)]
enum Op {
    Push(u64),

    Consume(u64),
}

fn ops_strategy() -> impl Strategy<Value = Vec<Op>> {
    prop::collection::vec(
        prop::strategy::Union::new_weighted(vec![
            (7, (0u64..64).prop_map(Op::Push).boxed()),
            (3, (1u64..4).prop_map(Op::Consume).boxed()),
        ]),
        1..200,
    )
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(512))]

    #[test]
    fn chaos_preserves_reconciliation_invariants(ops in ops_strategy()) {
        let mut buf = PlayerInputBuffer::new(16);
        let mut last_consumed: Option<u64> = None;
        for op in &ops {
            match *op {
                Op::Push(seq) => {
                    buf.push(PlayerInput::new(seq, seq as f64, vec![0u8; 8]));

                    prop_assert!(buf.pending_count() <= 16);
                }
                Op::Consume(n) => {
                    for _ in 0..n {
                        if let Some(input) = buf.consume() {

                            if let Some(prev) = last_consumed {
                                prop_assert!(input.seq > prev);
                            }
                            last_consumed = Some(input.seq);
                            prop_assert!(buf.consumed_count() >= 1);
                        }
                    }
                }
            }
        }

        while let Some(input) = buf.consume() {
            if let Some(prev) = last_consumed {
                prop_assert!(input.seq > prev, "pending seq {} not above consumed {}", input.seq, prev);
            }
            last_consumed = Some(input.seq);
        }
    }
}
