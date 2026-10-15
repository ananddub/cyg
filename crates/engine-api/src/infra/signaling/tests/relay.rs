use super::*;
use super::raw_ws::WsClient;
use serde_json::json;

#[tokio::test]
async fn relays_offer_and_answer_over_a_real_socket() {
    let (state, mut inbound) = test_state();
    let addr = spawn_plane(&state).await;
    let seat = mint_seat(&state);
    let mut ws = WsClient::open(addr, &signal_path(&seat)).await;

    // ICE config is the first frame: a client cannot gather without it.
    assert_eq!(
        ws.recv_json().await,
        json!({ "type": "ice", "config": [{ "urls": ["stun:stun.l.google.com:19302"] }] })
    );

    // Offer → data plane, seat stamped by the handler, not by the body.
    ws.send_text(r#"{"type":"offer","sdp":"v=0 fake offer"}"#).await;
    assert_eq!(
        next_signal(&mut inbound).await,
        SignalIn::Offer { seat: seat.clone(), sdp: "v=0 fake offer".into() }
    );

    // Answer → browser, through the hub exactly as a data plane would.
    assert!(state.signals.publish(&seat.id, SignalOut::Answer { sdp: "v=0 fake answer".into() }));
    assert_eq!(ws.recv_json().await, json!({ "type": "answer", "sdp": "v=0 fake answer" }));

    // Candidates both ways; the alias `sdpIndex` maps to the same field as
    // `sdpMLineIndex`.
    ws.send_text(
        r#"{"type":"candidate","candidate":"candidate:1 1 udp 1 10.0.0.1 5000 typ host","sdpMid":"0","sdpMLineIndex":0}"#,
    )
    .await;
    assert_eq!(
        next_signal(&mut inbound).await,
        SignalIn::Candidate {
            seat: seat.clone(),
            candidate: "candidate:1 1 udp 1 10.0.0.1 5000 typ host".into(),
            sdp_mid: Some("0".into()),
            sdp_index: Some(0),
        }
    );
    assert!(state.signals.publish(
        &seat.id,
        SignalOut::Candidate {
            candidate: "candidate:2 1 udp 2 10.0.0.2 5001 typ host".into(),
            sdp_mid: Some("0".into()),
            sdp_index: Some(0),
        }
    ));
    assert_eq!(
        ws.recv().await.unwrap(),
        r#"{"type":"candidate","candidate":"candidate:2 1 udp 2 10.0.0.2 5001 typ host","sdpMid":"0","sdpMLineIndex":0}"#
    );
    assert_eq!(state.signals.counts(), (3, 0), "ice, answer and candidate all arrived");

    // A client `{"type":"close"}` costs the data plane exactly one Close.
    ws.send_text(r#"{"type":"close"}"#).await;
    assert_eq!(next_signal(&mut inbound).await, SignalIn::Close { seat: seat.clone() });
    expect_quiet(&mut inbound).await;
    wait_peers(&state.signals, 0).await;
    assert!(
        !state.signals.publish(&seat.id, SignalOut::Error { message: "gone".into() }),
        "a seat whose socket closed must have no route left"
    );
}
