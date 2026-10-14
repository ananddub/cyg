//! End-to-end shape of the WebRTC signaling protocol: what the server does with a
//! frame a browser sends, and what a peer receives back.

use bytes::{Bytes, BytesMut};
use engine_voice::{
    rewrite_sender, SignalFrame, SignalKind, VoiceMesh, BROADCAST, SERVER_SENDER, SIGNAL_VERSION,
};

/// What a browser SDK puts in the `Join` body.
fn join_body(capabilities: &str) -> Bytes {
    Bytes::from(format!(r#"{{"version":{SIGNAL_VERSION},"capabilities":"{capabilities}"}}"#))
}

#[test]
fn peer_join_then_mesh_negotiation() {
    let mut mesh = VoiceMesh::new();
    let mut outbound: Vec<(String, SignalKind, u32, u32)> = Vec::new();

    // Two peers already in the room, on their own sockets.
    assert!(mesh.join(1, "sock-a"));
    assert!(mesh.join(2, "sock-b"));

    // A third client asks to join. It claims to be entity 99, which the server
    // ignores: the socket the frame arrived on decides the sender.
    let join = SignalFrame::new(SignalKind::Join, 99, SERVER_SENDER, join_body("opus,denoise"))
        .encode();
    let decoded = SignalFrame::decode(&join).unwrap();
    assert_eq!(decoded.kind, SignalKind::Join);
    assert_eq!(decoded.payload.as_ref(), br#"{"version":1,"capabilities":"opus,denoise"}"#);

    let registered = rewrite_sender(&join, 3);
    assert_eq!(SignalFrame::decode(&registered).unwrap().from, 3);
    assert!(mesh.join(3, "sock-c"));

    // The joiner gets the ICE server list privately; existing peers get the roster
    // notification so they know whom to negotiate with.
    let config = SignalFrame::new(
        SignalKind::Config,
        SERVER_SENDER,
        3,
        Bytes::from_static(br#"[{"urls":["stun:stun.l.google.com:19302"]}]"#),
    );
    outbound.push(("sock-c".to_string(), config.kind, config.from, config.to));

    for peer in mesh.peers_sorted() {
        if peer != 3 {
            let note = SignalFrame::new(
                SignalKind::PeerJoined,
                SERVER_SENDER,
                peer,
                Bytes::from_static(b"{\"entity_id\":3}"),
            );
            outbound.push((
                mesh.session_of(peer).unwrap().to_string(),
                note.kind,
                note.from,
                note.to,
            ));
        }
    }

    assert_eq!(
        outbound,
        vec![
            ("sock-c".to_string(), SignalKind::Config, SERVER_SENDER, 3),
            ("sock-a".to_string(), SignalKind::PeerJoined, SERVER_SENDER, 1),
            ("sock-b".to_string(), SignalKind::PeerJoined, SERVER_SENDER, 2),
        ]
    );

    // Peer 1 owns the lower id, so it initiates the offer to peer 3.
    assert!(VoiceMesh::should_initiate(1, 3));
    assert!(!VoiceMesh::should_initiate(3, 1));

    let offer = SignalFrame::new(
        SignalKind::Offer,
        1,
        3,
        Bytes::from_static(b"{\"sdp\":\"v=0\r\no=- 1 1 IN IP4 127.0.0.1\r\n\"}"),
    )
    .encode();

    let mut relayed: Vec<(String, Bytes)> = Vec::new();
    let delivered = mesh.route(1, 3, |target| {
        relayed.push((
            target.session_id.to_string(),
            rewrite_sender(&offer, 1),
        ));
    });
    assert!(delivered);
    assert_eq!(relayed.len(), 1);
    assert_eq!(relayed[0].0, "sock-c");

    let answered = SignalFrame::decode(&relayed[0].1).unwrap();
    assert_eq!(answered.kind, SignalKind::Offer);
    assert_eq!(answered.to, 3);

    // Peer 3 answers back to the single initiator.
    let answer = SignalFrame::new(
        SignalKind::Answer,
        3,
        1,
        Bytes::from_static(b"{\"sdp\":\"v=0\r\n\"}"),
    )
    .encode();
    let mut targets = Vec::new();
    assert!(mesh.route(3, 1, |t| targets.push(t.session_id.to_string())));
    assert_eq!(targets, vec!["sock-a".to_string()]);
    assert_eq!(SignalFrame::decode(&answer).unwrap().kind, SignalKind::Answer);
}

#[test]
fn forged_sender_slot_is_never_trusted() {
    let body = Bytes::from_static(b"{\"candidate\":true}");
    let mut wire = BytesMut::new();
    // Attacker claims to be entity 1 while sitting on entity 9's socket.
    SignalFrame::new(SignalKind::Ice, 1, 5, body.clone()).encode_into(&mut wire);
    let sent = wire.freeze();

    let corrected = rewrite_sender(&sent, 9);

    // Only the sender slot may differ; kind, target and body are relayed verbatim.
    assert_eq!(&corrected[..3], &sent[..3]);
    assert_eq!(&corrected[7..], &sent[7..]);
    assert_eq!(corrected.len(), sent.len());

    let decoded = SignalFrame::decode(&corrected).unwrap();
    assert_eq!(decoded.from, 9, "server must stamp its own session mapping");
    assert_eq!(decoded.to, 5);
    assert_eq!(decoded.payload, body);
}

#[test]
fn departed_peer_drops_frames_and_broadcast_skips_sender() {
    let mut mesh = VoiceMesh::new();
    mesh.join(1, "sock-a");
    mesh.join(2, "sock-b");
    mesh.join(3, "sock-c");

    let mut seen = Vec::new();
    mesh.route(2, BROADCAST, |t| seen.push((t.entity_id, t.session_id.to_string())));
    seen.sort_unstable();
    assert_eq!(
        seen,
        vec![(1, "sock-a".to_string()), (3, "sock-c".to_string())],
        "broadcast must not echo back to the sender"
    );

    // Socket close for peer 3: routed frames to it are dropped, not queued.
    assert_eq!(mesh.leave_session("sock-c"), Some(3));
    let payload = SignalFrame::new(SignalKind::Ice, 1, 3, Bytes::from_static(b"{}")).encode();
    let mut hits = 0;
    let delivered = mesh.route(1, 3, |_| hits += 1);
    assert!(!delivered);
    assert_eq!(hits, 0);
    assert_eq!(mesh.peer_count(), 2);
    assert_eq!(rewrite_sender(&payload, 1).len(), payload.len());
}

#[test]
fn audio_frames_are_never_mistaken_for_signaling() {
    // engine-voice Opus packets open with 'V' (0x56); signaling with 'W' (0x57).
    let opus = Bytes::from_static(b"\x56\x00\x00\x00\x07\x00\x01\x00\x00\x03\x84\x00\x02opus");
    assert!(!SignalFrame::is_signal(&opus));

    let signal =
        SignalFrame::new(SignalKind::Bye, 7, BROADCAST, Bytes::from_static(b"{}")).encode();
    assert!(SignalFrame::is_signal(&signal));
    assert_eq!(SignalFrame::peek_kind(&signal), Some(SignalKind::Bye));
    assert!(SignalKind::from_u8(6).is_none(), "reserved kinds must not decode");
    assert!(SignalKind::Ice.is_relayed());
    assert!(!SignalKind::Join.is_relayed());
    assert!(!SignalKind::Muted.is_relayed(), "mute verdicts are server-originated");
}

/// Byte-for-byte contract with the browser SDK. This vector was captured from
/// `examples/tank-3d/openoxide.js`:
/// `room.sendSignal(SignalKind.JOIN, 0xFFFFFFFF, {version:1, capabilities:"webrtc,opus"})`
/// If either side changes the layout, this fails here instead of at runtime in a
/// browser that has already asked the user for microphone access.
#[test]
fn rust_encoder_matches_browser_sdk_bytes() {
    const FROM_JS: &[u8] = &[
        0x57, 0x01, 0x01, 0x00, 0x00, 0x00, 0x00, 0xff, 0xff, 0xff, 0xff, 0x7b, 0x22, 0x76, 0x65,
        0x72, 0x73, 0x69, 0x6f, 0x6e, 0x22, 0x3a, 0x31, 0x2c, 0x22, 0x63, 0x61, 0x70, 0x61, 0x62,
        0x69, 0x6c, 0x69, 0x74, 0x69, 0x65, 0x73, 0x22, 0x3a, 0x22, 0x77, 0x65, 0x62, 0x72, 0x74,
        0x63, 0x2c, 0x6f, 0x70, 0x75, 0x73, 0x22, 0x7d,
    ];

    let mine = SignalFrame::new(
        SignalKind::Join,
        SERVER_SENDER,
        BROADCAST,
        Bytes::from_static(br#"{"version":1,"capabilities":"webrtc,opus"}"#),
    )
    .encode();
    assert_eq!(&mine[..], FROM_JS);

    let decoded = SignalFrame::decode(&Bytes::from_static(FROM_JS)).unwrap();
    assert_eq!(decoded.kind, SignalKind::Join);
    assert_eq!(decoded.from, SERVER_SENDER);
    assert_eq!(decoded.to, BROADCAST);
    assert_eq!(decoded.payload.as_ref(), br#"{"version":1,"capabilities":"webrtc,opus"}"#);
}
