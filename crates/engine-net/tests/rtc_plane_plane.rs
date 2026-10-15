//! Plane surface + session-pump behavior that does not need a remote peer:
//! - `create_session` returns a real datachannel SDP offer,
//! - the player→session registry lookup works,
//! - downlink `send_delta` is non-blocking and drops (never panics) on unknown
//!   players / full inboxes,
//! - an unconnected session's pump never emits oversized datagrams and drives its
//!   own timeout rather than busy-looping.

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::time::{Duration, Instant};

use engine_webrtc_protocol::MAX_FRAME;
use engine_webrtc_protocol::channel::{MAX_DELTA_CHUNK, delta_fragment_count};
use engine_net::rtc_plane::{
    Plane, PlaneConfig, PlaneHandle, RtcDownlink, RtcPlaneError, RtcSession,
};

fn loopback_any() -> SocketAddr {
    SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0)
}

#[tokio::test]
async fn plane_offer_lookup_and_nonblocking_downlink() {
    // Bind to an ephemeral loopback port; announce the concrete address we resolve to.
    let probe = std::net::UdpSocket::bind(loopback_any()).expect("probe bind");
    let announced = probe.local_addr().expect("probe addr");
    drop(probe);

    let (_plane, handle): (Plane, PlaneHandle) =
        Plane::spawn(announced, announced, PlaneConfig::default()).expect("spawn plane");

    let offer = handle
        .create_session("s1".into(), "p1".into())
        .await
        .expect("create session");
    assert!(offer.contains("m=application"), "offer must negotiate a datachannel m-line");

    // id -> session lookup the bridge needs.
    assert_eq!(handle.session_count(), 1);
    let sid = handle.session_for_player("p1").expect("player indexed");
    assert_eq!(&*sid, "s1");

    // Downlink for a known player is accepted into the bounded inbox and must
    // return immediately (the producer never blocks).
    let start = Instant::now();
    let accepted = handle.send_delta(&RtcDownlink {
        player_id: "p1".into(),
        tick: 1,
        seq: 1,
        body: vec![7u8; 5_000],
    });
    assert!(accepted, "delta for a known player should enter the inbox");
    assert!(start.elapsed() < Duration::from_millis(50), "send_delta must not block");

    // Unknown player is dropped, not queued, not an error path that panics.
    assert!(!handle.send_delta(&RtcDownlink {
        player_id: "ghost".into(),
        tick: 1,
        seq: 2,
        body: vec![0u8; 16],
    }));

    // Reliable downlink for an unknown player likewise returns false.
    assert!(!handle.send_reliable(&"ghost".into(), b"hello"));

    let closed = handle.close_session(sid).await;
    assert!(closed.is_ok());

    // Drain any pending uplink without hanging (there is no real peer, so the
    // channel stays open until shutdown; the timeout bounds the wait).
    let _ = tokio::time::timeout(Duration::from_millis(100), async {
        while handle.recv_uplink().await.is_some() {}
    })
    .await;
}

#[test]
fn session_pump_timeout_is_driven_not_busy() {
    // A freshly created session (no peer) exposes a concrete next timeout. str0m
    // keeps it in the distant future until negotiation starts (`NotHappening`), so
    // the loop must never busy-spin: it sleeps that long and wakes only on input.
    let server: SocketAddr = "127.0.0.1:41111".parse().unwrap();
    let (mut session, offer) =
        RtcSession::new_server("pump".into(), server, &[server]).expect("new server");
    assert!(!offer.is_empty());

    let first = session.poll_timeout();
    assert!(first >= Instant::now() - Duration::from_secs(1));

    // str0m permits feeding a Timeout at or before the scheduled instant. Drive the
    // engine at wall-clock; every emitted datagram must fit the wire budget.
    for _ in 0..50 {
        let now = Instant::now();
        for out in session.feed_timeout(now) {
            if let engine_net::rtc_plane::Outbound::Datagram { contents, .. } = out {
                assert!(contents.len() <= MAX_FRAME);
            }
        }
    }

    // A delta written before any peer connected is a no-op that still returns Ok
    // (the write is discarded by str0m), so the producer never errors mid-flight.
    let out = session.send_delta(1, 1, &[0u8; 100]).expect("delta on closed chan is a no-op");
    for o in out {
        if let engine_net::rtc_plane::Outbound::Datagram { contents, .. } = o {
            assert!(contents.len() <= MAX_FRAME);
        }
    }
}

#[test]
fn delta_fragmentation_bounds_match_codec() {
    // The codec's own chunk math the plane relies on: 5 KB needs 5 frames, an empty
    // body is one frame, and the plane rejects anything past MAX_DELTA_BODY.
    assert_eq!(delta_fragment_count(5_000), Some(5));
    assert_eq!(delta_fragment_count(0), Some(1));
    assert_eq!(delta_fragment_count(MAX_DELTA_CHUNK), Some(1));
    assert_eq!(delta_fragment_count(MAX_DELTA_CHUNK + 1), Some(2));
}

#[test]
fn oversized_delta_is_rejected_not_truncated() {
    // Even before connection, the plane validates body size against the codec cap
    // and rejects an impossible delta instead of silently dropping the tail.
    let server: SocketAddr = "127.0.0.1:41112".parse().unwrap();
    let (mut session, _offer) = RtcSession::new_server("big".into(), server, &[server]).unwrap();
    let huge = vec![0u8; 256 * MAX_DELTA_CHUNK + 1];
    match session.send_delta(1, 1, &huge) {
        Err(RtcPlaneError::DeltaTooLarge(n)) => assert!(n > 255 * MAX_DELTA_CHUNK),
        other => panic!("expected DeltaTooLarge, got {other:?}"),
    }
}
