use engine_webrtc_protocol::channel::{FragmentAssembler, pack_delta_frames};

#[test]
fn a_newest_wins_receiver_never_asks_for_the_delta_it_dropped() {
    let body: Vec<u8> = (0..4000u32).map(|i| i as u8).collect();
    let late = pack_delta_frames(50, 3000, &body).unwrap();
    let current = pack_delta_frames(51, 3001, &body).unwrap();

    let mut assembler = FragmentAssembler::default();
    for frame in &current {
        assembler.insert_frame(frame, 0);
    }
    assert_eq!(assembler.assembled(), 1);
    assert!(assembler.is_superseded(50));

    let before = assembler.assembled();
    for frame in &late {
        assert_eq!(assembler.insert_frame(frame, 1), None);
    }
    assert_eq!(assembler.stale(), late.len() as u64);
    assert_eq!(
        assembler.assembled(),
        before,
        "a superseded delta cannot apply twice"
    );
    assert_eq!(assembler.inflight(), 0);
}
