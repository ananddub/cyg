use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::time::{Duration, Instant};

use engine_net::rtc_plane::{
    Plane, PlaneConfig, PlaneHandle, RtcDownlink, RtcPlaneError, RtcSession,
};
use engine_webrtc_protocol::MAX_FRAME;
use engine_webrtc_protocol::channel::{MAX_DELTA_CHUNK, delta_fragment_count};

fn loopback_any() -> SocketAddr {
    SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0)
}

#[tokio::test]
async fn plane_offer_lookup_and_nonblocking_downlink() {
    let probe = std::net::UdpSocket::bind(loopback_any()).expect("probe bind");
    let announced = probe.local_addr().expect("probe addr");
    drop(probe);

    let (_plane, handle): (Plane, PlaneHandle) =
        Plane::spawn(announced, announced, PlaneConfig::default()).expect("spawn plane");

    let offer = handle
        .create_session("s1".into(), "p1".into())
        .await
        .expect("create session");
    assert!(
        offer.contains("m=application"),
        "offer must negotiate a datachannel m-line"
    );

    assert_eq!(handle.session_count(), 1);
    let sid = handle.session_for_player("p1").expect("player indexed");
    assert_eq!(&*sid, "s1");

    let start = Instant::now();
    let accepted = handle.send_delta(&RtcDownlink {
        player_id: "p1".into(),
        tick: 1,
        seq: 1,
        body: vec![7u8; 5_000],
    });
    assert!(accepted, "delta for a known player should enter the inbox");
    assert!(
        start.elapsed() < Duration::from_millis(50),
        "send_delta must not block"
    );

    assert!(!handle.send_delta(&RtcDownlink {
        player_id: "ghost".into(),
        tick: 1,
        seq: 2,
        body: vec![0u8; 16],
    }));

    assert!(!handle.send_reliable(&"ghost".into(), b"hello"));

    let closed = handle.close_session(sid).await;
    assert!(closed.is_ok());

    let _ = tokio::time::timeout(Duration::from_millis(100), async {
        while handle.recv_uplink().await.is_some() {}
    })
    .await;
}

#[test]
fn session_pump_timeout_is_driven_not_busy() {
    let server: SocketAddr = "127.0.0.1:41111".parse().unwrap();
    let (mut session, offer) =
        RtcSession::new_server("pump".into(), server, &[server]).expect("new server");
    assert!(!offer.is_empty());

    let first = session.poll_timeout();
    assert!(first >= Instant::now() - Duration::from_secs(1));

    for _ in 0..50 {
        let now = Instant::now();
        for out in session.feed_timeout(now) {
            if let engine_net::rtc_plane::Outbound::Datagram { contents, .. } = out {
                assert!(contents.len() <= MAX_FRAME);
            }
        }
    }

    let out = session
        .send_delta(1, 1, &[0u8; 100])
        .expect("delta on closed chan is a no-op");
    for o in out {
        if let engine_net::rtc_plane::Outbound::Datagram { contents, .. } = o {
            assert!(contents.len() <= MAX_FRAME);
        }
    }
}

#[test]
fn delta_fragmentation_bounds_match_codec() {
    assert_eq!(delta_fragment_count(5_000), Some(5));
    assert_eq!(delta_fragment_count(0), Some(1));
    assert_eq!(delta_fragment_count(MAX_DELTA_CHUNK), Some(1));
    assert_eq!(delta_fragment_count(MAX_DELTA_CHUNK + 1), Some(2));
}

#[test]
fn oversized_delta_is_rejected_not_truncated() {
    let server: SocketAddr = "127.0.0.1:41112".parse().unwrap();
    let (mut session, _offer) = RtcSession::new_server("big".into(), server, &[server]).unwrap();
    let huge = vec![0u8; 256 * MAX_DELTA_CHUNK + 1];
    match session.send_delta(1, 1, &huge) {
        Err(RtcPlaneError::DeltaTooLarge(n)) => assert!(n > 255 * MAX_DELTA_CHUNK),
        other => panic!("expected DeltaTooLarge, got {other:?}"),
    }
}
