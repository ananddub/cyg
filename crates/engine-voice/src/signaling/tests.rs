use bytes::{Bytes, BytesMut};

use super::*;

fn frame(kind: SignalKind, from: u32, to: u32, body: &str) -> SignalFrame {
    SignalFrame::new(kind, from, to, Bytes::copy_from_slice(body.as_bytes()))
}

#[test]
fn test_signal_codec_roundtrip() {
    let original = frame(SignalKind::Offer, 7, 12, r#"{"sdp":"v=0\r\n"}"#);
    let wire = original.encode();
    assert_eq!(wire.len(), original.wire_size());
    assert_eq!(wire[0], SIGNAL_MAGIC);

    let decoded = SignalFrame::decode(&wire).unwrap();
    assert_eq!(decoded, original);
    assert_eq!(decoded.payload.as_ref(), br#"{"sdp":"v=0\r\n"}"#);
}

#[test]
fn test_signal_rejects_bad_headers() {
    fn corrupt(wire: &Bytes, byte_index: usize, value: u8) -> Bytes {
        let mut copy = BytesMut::from(&wire[..]);
        copy[byte_index] = value;
        copy.freeze()
    }

    let wire = frame(SignalKind::Ice, 1, 2, "x").encode();

    assert_eq!(
        SignalFrame::decode(&corrupt(&wire, 0, b'V')),
        Err(SignalError::InvalidMagic(b'V'))
    );
    assert_eq!(
        SignalFrame::decode(&corrupt(&wire, 1, 9)),
        Err(SignalError::UnsupportedVersion(9))
    );
    assert_eq!(
        SignalFrame::decode(&corrupt(&wire, 2, 77)),
        Err(SignalError::UnknownKind(77))
    );
    assert_eq!(
        SignalFrame::decode(&wire.slice(0..5)),
        Err(SignalError::BufferTooShort {
            expected: SIGNAL_HEADER_LEN,
            actual: 5
        })
    );
}

#[test]
fn test_signal_rejects_oversized_body() {
    let huge = Bytes::from(vec![b'x'; MAX_SIGNAL_PAYLOAD + 1]);
    let wire = SignalFrame::new(SignalKind::Offer, 1, 2, huge).encode();
    assert_eq!(
        SignalFrame::decode(&wire),
        Err(SignalError::PayloadTooLarge {
            limit: MAX_SIGNAL_PAYLOAD,
            actual: MAX_SIGNAL_PAYLOAD + 1,
        })
    );
}

#[test]
fn test_signal_probe_distinguishes_audio_from_signaling() {
    let signal = frame(SignalKind::Join, 3, BROADCAST, "{}").encode();
    assert!(SignalFrame::is_signal(&signal));
    assert_eq!(SignalFrame::peek_kind(&signal), Some(SignalKind::Join));

    let audio = Bytes::from_static(b"V\x00\x00\x00\x01rest-of-an-opus-frame");
    assert!(!SignalFrame::is_signal(&audio));
    assert_eq!(SignalFrame::peek_kind(&audio), None);

    assert!(!SignalFrame::is_signal(b"W\x01\x02"));
}

#[test]
fn test_rewrite_sender_shares_payload() {
    let body = Bytes::from_static(b"{\"candidate\":\"spam\"}");
    let forged = SignalFrame::new(SignalKind::Ice, 999, 4, body.clone());
    let wire = forged.encode();
    let rewritten = rewrite_sender(&wire, 1);

    let decoded = SignalFrame::decode(&rewritten).unwrap();
    assert_eq!(decoded.from, 1);
    assert_eq!(decoded.to, 4);
    assert_eq!(decoded.kind, SignalKind::Ice);
    assert_eq!(decoded.payload, body);
}

#[test]
fn test_mesh_routing_targets_live_peers_only() {
    let mut mesh = VoiceMesh::new();
    mesh.join(1, "session-one");
    mesh.join(2, "session-two");
    mesh.join(3, "session-three");

    let mut direct = Vec::new();
    let hit = mesh.route(1, 2, |t| {
        direct.push((t.entity_id, t.session_id.to_string()))
    });
    assert!(hit);
    assert_eq!(direct, vec![(2, "session-two".to_string())]);

    let mut bcast = Vec::new();
    mesh.route(1, BROADCAST, |t| bcast.push(t.entity_id));
    bcast.sort_unstable();
    assert_eq!(bcast, vec![2, 3]);

    assert_eq!(mesh.leave(3).as_deref(), Some("session-three"));
    assert!(!mesh.route(1, 3, |_| panic!("must not route to a departed peer")));
}

#[test]
fn test_mesh_leave_session_keeps_maps_in_sync() {
    let mut mesh = VoiceMesh::new();
    mesh.join(5, "sock-5");
    assert_eq!(mesh.entity_of("sock-5"), Some(5));

    assert_eq!(mesh.leave_session("sock-5"), Some(5));
    assert_eq!(mesh.entity_of("sock-5"), None);
    assert_eq!(mesh.session_of(5), None);
    assert_eq!(mesh.peer_count(), 0);

    mesh.join(5, "sock-5-new");
    mesh.join(6, "sock-6");
    assert_eq!(mesh.entity_of("sock-5"), None);
    assert_eq!(mesh.session_of(5), Some("sock-5-new"));
    assert_eq!(mesh.peers_sorted(), vec![5, 6]);
}

#[test]
fn test_initiator_rule_is_symmetric() {
    assert!(VoiceMesh::should_initiate(1, 2));
    assert!(!VoiceMesh::should_initiate(2, 1));
    assert!(!VoiceMesh::should_initiate(4, 4));
}
