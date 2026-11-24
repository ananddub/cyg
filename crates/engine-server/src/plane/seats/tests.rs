use super::registry::SeatRegistry;
use super::token::{SeatError, b64url_decode, b64url_encode};
use crate::netcode::admission::{self, DenyReason, Lane};

fn reg() -> SeatRegistry {
    SeatRegistry::new("test-secret")
}

#[test]
fn mint_verify_roundtrip() {
    let r = reg();
    let token = r.mint("tank-3d", 300);
    let grant = r.verify(&token, None).expect("valid token should verify");
    assert_eq!(grant.room, "tank-3d");
}

#[test]
fn wrong_secret_rejected() {
    let token = reg().mint("tank-3d", 300);
    let err = SeatRegistry::new("other-secret")
        .verify(&token, None)
        .expect_err("wrong secret must be rejected");
    assert_eq!(err, SeatError::Bad);
}

#[test]
fn expired_token_rejected() {
    let r = reg();
    // ttl 0 -> exp == now, and verify rejects exp <= now
    let token = r.mint("tank-3d", 0);
    let err = r
        .verify(&token, None)
        .expect_err("expired token must be rejected");
    assert_eq!(err, SeatError::Expired);
}

#[test]
fn room_match_enforced() {
    let r = reg();
    let token = r.mint("tank-3d", 300);
    let err = r
        .verify(&token, Some("other-room"))
        .expect_err("room mismatch must be rejected");
    assert_eq!(err, SeatError::RoomMismatch);
}

#[test]
fn room_match_accepts_correct_room() {
    let r = reg();
    let token = r.mint("tank-3d", 300);
    let grant = r
        .verify(&token, Some("tank-3d"))
        .expect("matching room should verify");
    assert_eq!(grant.room, "tank-3d");
}

#[test]
fn replay_deny() {
    let r = reg();
    let token = r.mint("tank-3d", 300);
    let first = r.verify(&token, Some("tank-3d"));
    assert!(first.is_ok(), "first use should succeed");
    let err = r
        .verify(&token, Some("tank-3d"))
        .expect_err("second use must be replay-denied");
    assert_eq!(err, SeatError::Replayed);
}

#[test]
fn join_frame_parses_only_the_canonical_envelope() {
    let bad_frames: &[&[u8]] = &[
        b"\x20\x03",           // truncated: len says 3 but no payload
        b"\x0a\x03abc",        // wrong op
        b"\x20\x02abc",        // len mismatch
        b"\x20\x04ab",         // len mismatch
        b"",                     // empty
        b"\x20",                // only op
    ];
    for bad in bad_frames {
        assert!(
            super::token::join_token(bad).is_none(),
            "must reject join frame {bad:?}"
        );
    }
    let token = reg().mint("tank-3d", 300);
    let mut frame = vec![0x20, token.len() as u8];
    frame.extend_from_slice(token.as_bytes());
    assert!(super::token::join_token(&frame).is_some());
}

#[test]
fn admission_rejects_bad_frame_for_both_udp_lanes() {
    let r = reg();
    for lane in [Lane::UdpHost, Lane::UdpPlane] {
        let err = admission::admit_join_frame(lane, &r, b"\x20\x01x", None)
            .expect_err("bad frame must be denied");
        assert_eq!(err, DenyReason::BadToken);
    }
}

#[test]
fn admission_accepts_valid_frame_for_both_udp_lanes() {
    for lane in [Lane::UdpHost, Lane::UdpPlane] {
        let r = reg();
        let token = r.mint("tank-3d", 300);
        let mut frame = vec![0x20, token.len() as u8];
        frame.extend_from_slice(token.as_bytes());
        let session = admission::admit_join_frame(lane, &r, &frame, None)
            .expect("valid frame should be admitted");
        assert_eq!(session.room, "tank-3d");
        assert_eq!(session.lane, lane);
    }
}

#[test]
fn admission_enforces_room_match_on_plane_lane() {
    let r = reg();
    let token = r.mint("tank-3d", 300);
    let mut frame = vec![0x20, token.len() as u8];
    frame.extend_from_slice(token.as_bytes());
    let err = admission::admit_join_frame(Lane::UdpPlane, &r, &frame, Some("other-room"))
        .expect_err("room mismatch must be denied");
    assert_eq!(err, DenyReason::RoomMismatch);
}

#[test]
fn admission_replay_denied() {
    let r = reg();
    let token = r.mint("tank-3d", 300);
    let mut frame = vec![0x20, token.len() as u8];
    frame.extend_from_slice(token.as_bytes());
    admission::admit_join_frame(Lane::UdpHost, &r, &frame, None).expect("first admit");
    let err = admission::admit_join_frame(Lane::UdpHost, &r, &frame, None)
        .expect_err("replay must be denied");
    assert_eq!(err, DenyReason::Replayed);
}
