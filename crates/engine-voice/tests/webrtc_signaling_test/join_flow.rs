use bytes::Bytes;
use engine_voice::{SERVER_SENDER, SignalFrame, SignalKind, VoiceMesh, rewrite_sender};

use crate::harness::join_body;

#[test]
fn peer_join_then_mesh_negotiation() {
    let mut mesh = VoiceMesh::new();
    let mut outbound: Vec<(String, SignalKind, u32, u32)> = Vec::new();

    assert!(mesh.join(1, "sock-a"));
    assert!(mesh.join(2, "sock-b"));

    let join = SignalFrame::new(
        SignalKind::Join,
        99,
        SERVER_SENDER,
        join_body("opus,denoise"),
    )
    .encode();
    let decoded = SignalFrame::decode(&join).unwrap();
    assert_eq!(decoded.kind, SignalKind::Join);
    assert_eq!(
        decoded.payload.as_ref(),
        br#"{"version":1,"capabilities":"opus,denoise"}"#
    );

    let registered = rewrite_sender(&join, 3);
    assert_eq!(SignalFrame::decode(&registered).unwrap().from, 3);
    assert!(mesh.join(3, "sock-c"));

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
            (
                "sock-a".to_string(),
                SignalKind::PeerJoined,
                SERVER_SENDER,
                1
            ),
            (
                "sock-b".to_string(),
                SignalKind::PeerJoined,
                SERVER_SENDER,
                2
            ),
        ]
    );

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
        relayed.push((target.session_id.to_string(), rewrite_sender(&offer, 1)));
    });
    assert!(delivered);
    assert_eq!(relayed.len(), 1);
    assert_eq!(relayed[0].0, "sock-c");

    let answered = SignalFrame::decode(&relayed[0].1).unwrap();
    assert_eq!(answered.kind, SignalKind::Offer);
    assert_eq!(answered.to, 3);

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
    assert_eq!(
        SignalFrame::decode(&answer).unwrap().kind,
        SignalKind::Answer
    );
}
