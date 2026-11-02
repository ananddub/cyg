use bytes::{Bytes, BytesMut};
use engine_voice::{BROADCAST, SERVER_SENDER, SignalFrame, SignalKind, VoiceMesh, rewrite_sender};

#[test]
fn forged_sender_slot_is_never_trusted() {
    let body = Bytes::from_static(b"{\"candidate\":true}");
    let mut wire = BytesMut::new();

    SignalFrame::new(SignalKind::Ice, 1, 5, body.clone()).encode_into(&mut wire);
    let sent = wire.freeze();

    let corrected = rewrite_sender(&sent, 9);

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
    mesh.route(2, BROADCAST, |t| {
        seen.push((t.entity_id, t.session_id.to_string()))
    });
    seen.sort_unstable();
    assert_eq!(
        seen,
        vec![(1, "sock-a".to_string()), (3, "sock-c".to_string())],
        "broadcast must not echo back to the sender"
    );

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
    let opus = Bytes::from_static(b"\x56\x00\x00\x00\x07\x00\x01\x00\x00\x03\x84\x00\x02opus");
    assert!(!SignalFrame::is_signal(&opus));

    let signal =
        SignalFrame::new(SignalKind::Bye, 7, BROADCAST, Bytes::from_static(b"{}")).encode();
    assert!(SignalFrame::is_signal(&signal));
    assert_eq!(SignalFrame::peek_kind(&signal), Some(SignalKind::Bye));
    assert!(
        SignalKind::from_u8(6).is_none(),
        "reserved kinds must not decode"
    );
    assert!(SignalKind::Ice.is_relayed());
    assert!(!SignalKind::Join.is_relayed());
    assert!(
        !SignalKind::Muted.is_relayed(),
        "mute verdicts are server-originated"
    );
}

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
    assert_eq!(
        decoded.payload.as_ref(),
        br#"{"version":1,"capabilities":"webrtc,opus"}"#
    );
}
