use crate::frame;
use engine_webrtc_protocol::channel::{
    FragmentAssembler, MAX_DELTA_CHUNK, MAX_INFLIGHT_BYTES, MAX_INFLIGHT_DELTAS,
};

#[test]
fn a_delivery_releases_the_older_partial() {
    const MAXC: usize = MAX_DELTA_CHUNK;
    let mut assembler = FragmentAssembler::default();
    assert_eq!(
        assembler.insert_frame(&frame(10, 1, 0, 2, &[0xaa; MAXC]), 0),
        None
    );
    assert_eq!(assembler.inflight(), 1);

    assert_eq!(
        assembler.insert_frame(&frame(11, 2, 0, 2, &[0xbb; MAXC]), 1),
        None
    );
    assert_eq!(assembler.inflight(), 2, "both deltas are in flight");

    let delta = assembler
        .insert_frame(&frame(11, 2, 1, 2, &[0xbc]), 2)
        .expect("11 closes");
    let mut expected = vec![0xbb; MAXC];
    expected.push(0xbc);
    assert_eq!((delta.seq, delta.tick, delta.body), (11, 2, expected));
    assert_eq!(assembler.inflight(), 0, "10 was superseded by the delivery");
    assert_eq!(assembler.evicted(), 1);
    assert_eq!(assembler.buffered_bytes(), 0);

    assert_eq!(
        assembler.insert_frame(&frame(10, 1, 1, 2, &[0xab]), 3),
        None
    );
    assert_eq!(assembler.stale(), 1);
    assert_eq!(assembler.assembled(), 1);
}

#[test]
fn the_inflight_window_bounds_a_spray_of_half_deltas() {
    let mut assembler = FragmentAssembler::default();
    for seq in 0..40u16 {
        let wire = frame(seq, seq as u32, 0, 2, &[0x41; MAX_DELTA_CHUNK]);
        assembler.insert_frame(&wire, seq as u64);
        assert!(
            assembler.inflight() <= MAX_INFLIGHT_DELTAS,
            "the window may not exceed {MAX_INFLIGHT_DELTAS} deltas"
        );
        assert!(assembler.buffered_bytes() <= MAX_INFLIGHT_BYTES);
    }
    assert_eq!(assembler.evicted(), 40 - MAX_INFLIGHT_DELTAS as u64);

    let last = 39u16;
    let delta = assembler
        .insert_frame(&frame(last, last as u32, 1, 2, &[0x42]), 100)
        .expect("the newest delta survives the window");
    assert_eq!(delta.seq, last);
    let mut expected = vec![0x41u8; MAX_DELTA_CHUNK];
    expected.push(0x42);
    assert_eq!(delta.body, expected);

    assert_eq!(
        assembler.insert_frame(&frame(0, 0, 1, 2, &[0x42]), 101),
        None
    );
    assert_eq!(assembler.stale(), 1, "0 is behind what has been delivered");
}
