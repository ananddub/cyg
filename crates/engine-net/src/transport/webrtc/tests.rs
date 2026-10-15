//! Refusal and accounting at the delta/MTU boundary, where no real peer is
//! needed to prove what the transport reports.

use engine_webrtc_protocol::{MAX_DELTA_BODY, MAX_FRAME};

use super::WebRtcServer;

const FAT: usize = MAX_DELTA_BODY + 1;

/// The refusal happens at the codec boundary, before any session lookup, so
/// an unsendable body is never partially written either.
#[test]
fn a_delta_needing_256_fragments_is_refused_and_counted() {
    let mut server = WebRtcServer::new(vec!["127.0.0.1:7791".parse().unwrap()]);
    let body = vec![0x7fu8; FAT];
    let err = server
        .send_delta("nobody", 1, 1, &body)
        .expect_err("more than MAX_FRAGMENTS cannot be carried");
    assert!(err.to_string().contains("fragments"));
    assert_eq!(server.stats().deltas_refused_oversized, 1);
    assert_eq!(server.stats().frames_sent, 0, "nothing reached a lane");
}

#[test]
fn broadcast_counts_a_refused_delta_for_every_session_at_once() {
    let mut server = WebRtcServer::new(vec!["127.0.0.1:7792".parse().unwrap()]);
    assert_eq!(server.broadcast_delta(3, 4, &vec![0u8; FAT]), 0);
    assert_eq!(server.stats().deltas_refused_oversized, 1);
}

/// A legal delta with no session is reported as skipped, not as sent: the
/// host must fall back rather than assume the client got its tick.
#[test]
fn a_delta_for_an_unknown_session_is_not_reported_as_delivered() {
    let mut server = WebRtcServer::new(vec![]);
    let report = server.send_delta("ghost", 9, 9, &[1, 2, 3]).unwrap();
    assert_eq!(report.fragments, 1);
    assert_eq!(report.frames_accepted, 0);
    assert!(!report.delivered());
    assert_eq!(server.stats().frames_dropped_no_channel, 1);
}

#[test]
fn raw_writes_over_the_mtu_are_refused_not_truncated() {
    let mut server = WebRtcServer::new(vec![]);
    let too_big = vec![0u8; MAX_FRAME + 1];
    assert!(!server.send_unreliable("nobody", &too_big));
    assert!(!server.send_reliable("nobody", &too_big));
    assert_eq!(server.broadcast_unreliable(&too_big), 0);
    assert_eq!(server.broadcast_reliable(&too_big), 0);
    assert_eq!(server.stats().frames_refused_oversize, 4);
}

#[test]
fn seq_watermarks_are_per_session_and_forward_only() {
    let server = WebRtcServer::new(vec![]);
    assert_eq!(server.last_input_seq("nobody"), None);
    assert_eq!(server.acked_event_seq("nobody"), None);
    assert_eq!(server.outbound_backlog("nobody"), 0);
    assert!(!server.is_connected("nobody"));
}
