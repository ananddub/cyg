use crate::netcode::*;
use crate::rooms::{GameRoom, SessionSeqs};

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
    room.sessions.insert("sess_alice".into(), 1);
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
    let mut payload = sample(0, 0.0, 0.0, 1.0, 0.0);

    assert_eq!(
        apply_input(&mut room, &cfg, 0.033, "sess_bob", 2, 1, &mut payload),
        Err(InputRejection::NotSeated)
    );
    assert!(room.seqs.is_empty(), "a refused input moved the baseline");
    assert_eq!(
        apply_input(&mut room, &cfg, 0.033, "sess_alice", 1, 1, &mut payload),
        Ok(())
    );
}

#[test]
fn a_non_finite_or_absurd_axis_is_sanitized_never_rejected() {
    let cfg = engine_config::GameConfig::default();
    let mut room = seated_room();

    // NaN lands on the clamp floor, absurd magnitudes on the domain edge —
    // the frame the guest reads is always legal (Colyseus `sanitize` rule).
    let mut nan = sample(0, 0.0, f32::NAN, 0.0, 0.0);
    apply_input(&mut room, &cfg, 0.033, "sess_alice", 1, 5, &mut nan).unwrap();
    assert_eq!(
        f32::from_le_bytes(nan[8..12].try_into().unwrap()),
        -std::f32::consts::PI,
        "NaN aim lane reached the guest"
    );

    let mut absurd = sample(0, 0.0, MAX_INPUT_AXIS * 2.0, 127.0, 0.0);
    apply_input(&mut room, &cfg, 0.033, "sess_alice", 1, 6, &mut absurd).unwrap();
    assert_eq!(
        f32::from_le_bytes(absurd[8..12].try_into().unwrap()),
        std::f32::consts::PI,
        "out-of-range aim lane reached the guest"
    );
    assert_eq!(
        f32::from_le_bytes(absurd[12..16].try_into().unwrap()),
        1.0,
        "speed-hack move lane reached the guest"
    );
}

#[test]
fn a_seq_beyond_the_window_re_anchors_instead_of_locking_the_seat_out() {
    let cfg = engine_config::GameConfig::default();
    let mut room = seated_room();
    let mut payload = sample(0, 0.0, 0.0, 1.0, 0.0);
    let jumped = 5u16.wrapping_add(MAX_SEQ_AHEAD).wrapping_add(4000);

    assert_eq!(
        apply_input(&mut room, &cfg, 0.033, "sess_alice", 1, 5, &mut payload),
        Ok(())
    );
    assert_eq!(
        apply_input(&mut room, &cfg, 0.033, "sess_alice", 1, jumped, &mut payload),
        Ok(())
    );
    assert_eq!(room.seqs[&1].input, jumped);
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
    assert_eq!(
        batch_sample_count(&[OP_INPUT_BATCH, 0]),
        None,
        "empty batch"
    );
    assert_eq!(
        batch_sample_count(&batch(MAX_BATCHED_INPUTS + 1)),
        None,
        "over the per-packet cap"
    );
}

#[test]
fn staged_inputs_coalesce_newest_wins_per_player() {
    let mut room = seated_room();
    room.input_coalesce = true;

    deliver_input(&mut room, 1, &sample(0, 0.0, 0.0, 1.0, 0.0));
    deliver_input(&mut room, 1, &sample(0, 0.0, 0.0, 2.0, 0.0));
    deliver_input(&mut room, 2, &sample(0, 0.0, 0.0, 3.0, 0.0));

    assert_eq!(
        room.input_inbox.len(),
        2,
        "two players staged, not three frames"
    );
    assert_eq!(
        room.input_inbox[0].1,
        sample(0, 0.0, 0.0, 2.0, 0.0),
        "the player's newest frame replaced the earlier one"
    );
}

#[test]
fn flushing_the_inbox_drops_departed_players_and_clears_it() {
    let mut room = seated_room();
    room.input_coalesce = true;
    deliver_input(&mut room, 1, &sample(0, 0.0, 0.0, 1.0, 0.0));
    deliver_input(&mut room, 9, &sample(0, 0.0, 0.0, 1.0, 0.0));
    room.sessions.remove("sess_alice");

    room.flush_input_inbox();
    assert!(
        room.input_inbox.is_empty(),
        "a flush consumes the staging area"
    );
}

#[test]
fn coalesce_off_delivers_immediately_without_staging() {
    let cfg = engine_config::GameConfig::default();
    let mut room = seated_room();
    assert!(!room.input_coalesce, "default dispatch is immediate");

    let mut payload = sample(0, 0.0, 0.0, 1.0, 0.0);
    assert_eq!(
        apply_input(&mut room, &cfg, 0.033, "sess_alice", 1, 1, &mut payload),
        Ok(())
    );
    assert!(
        room.input_inbox.is_empty(),
        "immediate dispatch never stages"
    );
}

#[test]
fn an_empty_tick_holds_the_last_input_while_connected_and_stops_on_drop() {
    let cfg = engine_config::GameConfig::default();
    let mut room = seated_room();
    let mut payload = sample(0, 0.0, 0.0, 1.0, 0.0);
    apply_input(&mut room, &cfg, 0.033, "sess_alice", 1, 1, &mut payload).unwrap();

    // The tick that consumed the real input synthesizes nothing.
    room.synthesize_idle_inputs(|_| true);
    assert!(room.input_inbox.is_empty());

    // The NEXT tick with no input coasts on the held frame.
    room.synthesize_idle_inputs(|_| true);
    assert_eq!(room.input_inbox.len(), 1);
    assert_eq!(room.input_inbox[0].0, 1);
    assert_eq!(room.input_inbox[0].1.as_slice(), payload.as_slice());

    // A dropped connection stops the hold — no walking ghost.
    room.input_inbox.clear();
    room.synthesize_idle_inputs(|_| false);
    assert!(room.input_inbox.is_empty(), "a disconnected seat kept walking");
}

#[test]
fn a_held_schema_frame_bumps_its_lane_seq_but_never_the_reconcile_ack() {
    let cfg = engine_config::GameConfig::default();
    let mut room = seated_room();

    // Schema-lane frame: [0x05][7]["__input"][seq u16 LE][frame...], seq 10.
    let mut schema_frame = Vec::from(&b"\x05\x07__input\x0a\x00\x01\x02\x03"[..]);
    room.last_input.insert(1, schema_frame.clone());
    room.seqs.insert(
        1,
        SessionSeqs {
            input: 42,
            acked_event: 0,
            stamp: 0,
        },
    );

    room.synthesize_idle_inputs(|_| true);
    assert_eq!(room.input_inbox.len(), 1);
    let held = &room.input_inbox[0].1;
    assert_eq!(&held[..9], &schema_frame[..9], "header unchanged");
    assert_eq!(&held[11..], &schema_frame[11..], "payload unchanged");
    assert_eq!(u16::from_le_bytes([held[9], held[10]]), 11, "lane seq bumped");
    assert_eq!(room.seqs[&1].input, 42, "reconcile ack untouched");
}

#[test]
fn the_delta_tail_carries_the_phase_d_rtt_echo() {
    let mut out = Vec::new();
    wrap_delta_into(&mut out, 7, b"body", 0x1234, 0x0000_00FF, 0xBEEF);
    let n = out.len();
    assert_eq!(&out[n - 8..n - 6], &0x1234u16.to_le_bytes(), "acked_seq");
    assert_eq!(&out[n - 6..n - 2], &0x0000_00FFu32.to_le_bytes(), "server_tick");
    assert_eq!(&out[n - 2..], &0xBEEFu16.to_le_bytes(), "client_stamp");

    // Per-session patch rewrites id + acked + stamp on the prebuilt frame.
    patch_wrap_into(&mut out, 9, 0x0042, 0xCAFE);
    assert_eq!(&out[6..14], &9i64.to_le_bytes());
    assert_eq!(&out[n - 8..n - 6], &0x0042u16.to_le_bytes());
    assert_eq!(&out[n - 2..], &0xCAFEu16.to_le_bytes());
}
