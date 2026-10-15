use crate::{feed, frame, vectors::DELTA_FRAG_0, vectors::DELTA_FRAG_1};
use engine_webrtc_protocol::channel::{FRAGMENT_TTL_MS, FragmentAssembler};

#[test]
fn a_stale_half_delta_is_cleaned_up_after_the_ttl() {
    let mut assembler = FragmentAssembler::default();
    assert_eq!(feed(&mut assembler, &DELTA_FRAG_0, 0), None);

    assert!(!assembler.cleanup(0));
    assert!(!assembler.cleanup(FRAGMENT_TTL_MS - 1));
    assert!(assembler.cleanup(FRAGMENT_TTL_MS + 1));
    assert_eq!(assembler.newest_seq(), None);
    assert_eq!(assembler.inflight(), 0);
    assert_eq!(
        assembler.buffered_bytes(),
        0,
        "an expired slot returns its bytes"
    );

    assert_eq!(
        feed(&mut assembler, &DELTA_FRAG_1, FRAGMENT_TTL_MS + 2),
        None
    );
    assert_eq!(
        feed(&mut assembler, &DELTA_FRAG_0, FRAGMENT_TTL_MS + 3),
        Some(vec![0xde, 0xad, 0xbe, 0xef, 0x00, 0x11, 0x22, 0x33])
    );
}

/// TTL is per delta: a peer making progress on one tick must not lose the
/// stalled one next to it, and vice versa.
#[test]
fn ttl_expiry_sweeps_only_the_stalled_deltas() {
    let mut assembler = FragmentAssembler::default();
    assert_eq!(assembler.insert_frame(&frame(100, 10, 0, 3, &[1]), 0), None);
    assert_eq!(assembler.insert_frame(&frame(101, 11, 0, 3, &[2]), 0), None);
    assert_eq!(assembler.inflight(), 2);

    assert_eq!(
        assembler.insert_frame(&frame(101, 11, 1, 3, &[3]), 2_500),
        None
    );
    assert_eq!(
        assembler.assembled(),
        0,
        "neither delta has all three chunks yet"
    );

    assert!(assembler.cleanup(3_501));
    assert_eq!(assembler.inflight(), 1);
    assert_eq!(assembler.newest_seq(), None);
    assert_eq!(
        assembler.buffered_bytes(),
        2,
        "only the stalled delta returned its bytes"
    );
    assert!(!assembler.cleanup(4_000), "the live slot is not stalled");

    assert_eq!(
        assembler
            .insert_frame(&frame(101, 11, 2, 3, &[4]), 4_500)
            .unwrap()
            .body,
        vec![2, 3, 4]
    );
    assert_eq!(assembler.newest_seq(), Some(101));
    assert_eq!(
        assembler.insert_frame(&frame(100, 10, 0, 3, &[1]), 4_600),
        None
    );
    assert_eq!(assembler.stale(), 1, "100 is behind what was delivered");
}
