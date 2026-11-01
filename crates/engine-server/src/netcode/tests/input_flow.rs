use crate::netcode::*;
use crate::rooms::GameRoom;

fn sample(flags: u32, yaw: f32, pitch: f32, mx: f32, mz: f32) -> [u8; 20] {
    let mut p = [0u8; 20];
    p[0..4].copy_from_slice(&flags.to_le_bytes());
    p[4..8].copy_from_slice(&yaw.to_le_bytes());
    p[8..12].copy_from_slice(&pitch.to_le_bytes());
    p[12..16].copy_from_slice(&mx.to_le_bytes());
    p[16..20].copy_from_slice(&mz.to_le_bytes());
    p
}

fn seated_room() -> GameRoom {
    let mut room = GameRoom::new_relay();
    room.sessions.insert("sess_alice".to_string(), 1);
    room
}

fn batch(count: usize) -> Vec<u8> {
    let mut out = vec![OP_INPUT_BATCH, count as u8, 0];
    out.extend(
        std::iter::repeat_n(sample(0, 0.0, 0.0, 1.0, 0.0), count)
            .flat_map(|p| [&[0u8; 2][..], &p[..]].concat()),
    );
    out
}

#[test]
fn an_input_for_a_seat_the_session_does_not_hold_is_refused() {
    let cfg = engine_config::GameConfig::default();
    let mut room = seated_room();
    let payload = sample(0, 0.0, 0.0, 1.0, 0.0);

    assert_eq!(
        apply_input(&mut room, &cfg, 0.033, "sess_bob", 2, 1, &payload),
        Err(InputRejection::NotSeated)
    );
    assert!(
        room.last_input_seq.is_empty(),
        "a refused input moved the baseline"
    );
    assert_eq!(
        apply_input(&mut room, &cfg, 0.033, "sess_alice", 1, 1, &payload),
        Ok(())
    );
}

#[test]
fn a_non_finite_or_absurd_axis_is_refused_before_the_guest_sees_it() {
    let cfg = engine_config::GameConfig::default();
    let mut room = seated_room();

    for (label, bad) in [
        ("NaN", f32::NAN),
        ("Inf", f32::INFINITY),
        ("out of range", MAX_INPUT_AXIS * 2.0),
    ] {
        let payload = sample(0, 0.0, bad, 0.0, 0.0);
        assert!(!validate_input_payload(&payload), "{label} lane accepted");
        assert_eq!(
            apply_input(&mut room, &cfg, 0.033, "sess_alice", 1, 5, &payload),
            Err(InputRejection::Malformed),
            "{label} reached the guest"
        );
    }
    assert!(
        room.last_input_seq.is_empty(),
        "a refused input moved the baseline"
    );
}

#[test]
fn a_seq_beyond_the_window_re_anchors_instead_of_locking_the_seat_out() {
    let cfg = engine_config::GameConfig::default();
    let mut room = seated_room();
    let payload = sample(0, 0.0, 0.0, 1.0, 0.0);
    let jumped = 5u16.wrapping_add(MAX_SEQ_AHEAD).wrapping_add(4000);

    assert_eq!(
        apply_input(&mut room, &cfg, 0.033, "sess_alice", 1, 5, &payload),
        Ok(())
    );
    assert_eq!(
        apply_input(&mut room, &cfg, 0.033, "sess_alice", 1, jumped, &payload),
        Ok(())
    );
    assert_eq!(room.last_input_seq["sess_alice"], jumped);
}

#[test]
fn a_batch_that_contradicts_its_header_is_refused_whole() {
    let good = batch(3);
    assert_eq!(batch_sample_count(&good), Some(3));

    let mut truncated = good.clone();
    truncated.truncate(good.len() - 5);
    assert_eq!(batch_sample_count(&truncated), None, "partial sample");

    let mut lying_count = good.clone();
    lying_count[1] = 4;
    assert_eq!(
        batch_sample_count(&lying_count),
        None,
        "count byte disagrees"
    );

    assert_eq!(batch_sample_count(&batch(1)), Some(1));
    assert_eq!(batch_sample_count(&[]), None);
    assert_eq!(batch_sample_count(&[OP_INPUT_BATCH, 0]), None, "empty batch");
    assert_eq!(
        batch_sample_count(&batch(MAX_BATCHED_INPUTS + 1)),
        None,
        "over the per-packet cap"
    );
}
