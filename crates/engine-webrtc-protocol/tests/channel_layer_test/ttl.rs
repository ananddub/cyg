use crate::{feed, frame};
use engine_webrtc_protocol::channel::{FRAGMENT_TTL_MS, MAX_DELTA_CHUNK, FragmentAssembler};

#[test]
fn a_stale_half_delta_is_cleaned_up_after_the_ttl() {
    const MAXC: usize = MAX_DELTA_CHUNK;
    let chunk0 = vec![0xde; MAXC];
    let chunk1 = vec![0xad, 0xbe, 0xef, 0x00];

    let mut assembler = FragmentAssembler::default();
    assert_eq!(
        feed(&mut assembler, &frame(65535, 8, 0, 2, &chunk0), 0),
        None
    );

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
        feed(
            &mut assembler,
            &frame(65535, 8, 1, 2, &chunk1),
            FRAGMENT_TTL_MS + 2
        ),
        None
    );
    let mut expected = chunk0.clone();
    expected.extend_from_slice(&chunk1);
    assert_eq!(
        feed(
            &mut assembler,
            &frame(65535, 8, 0, 2, &chunk0),
            FRAGMENT_TTL_MS + 3
        ),
        Some(expected)
    );
}

#[test]
fn ttl_expiry_sweeps_only_the_stalled_deltas() {
    const MAXC: usize = MAX_DELTA_CHUNK;
    let mut assembler = FragmentAssembler::default();
    assert_eq!(
        assembler.insert_frame(&frame(100, 10, 0, 3, &[1; MAXC]), 0),
        None
    );
    assert_eq!(
        assembler.insert_frame(&frame(101, 11, 0, 3, &[2; MAXC]), 0),
        None
    );
    assert_eq!(assembler.inflight(), 2);

    assert_eq!(
        assembler.insert_frame(&frame(101, 11, 1, 3, &[3; MAXC]), 2_500),
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
        2 * MAXC,
        "only the stalled delta returned its bytes"
    );
    assert!(!assembler.cleanup(4_000), "the live slot is not stalled");

    assert_eq!(
        assembler
            .insert_frame(&frame(101, 11, 2, 3, &[4]), 4_500)
            .unwrap()
            .body,
        {
            let mut body = vec![2; MAXC];
            body.extend_from_slice(&vec![3; MAXC]);
            body.push(4);
            body
        }
    );
    assert_eq!(assembler.newest_seq(), Some(101));
    assert_eq!(
        assembler.insert_frame(&frame(100, 10, 0, 3, &[1; MAXC]), 4_600),
        None
    );
    assert_eq!(assembler.stale(), 1, "100 is behind what was delivered");
}
