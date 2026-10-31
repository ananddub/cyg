use super::raw_ws::WsClient;
use super::*;
use crate::infra::signaling::wire::ServerFrame;
use crate::utils::signaling::parse_ice;

#[tokio::test]
async fn a_malformed_frame_errors_without_killing_the_session() {
    let (state, mut inbound) = test_state();
    let addr = spawn_plane(&state).await;
    let seat = mint_seat(&state);
    let mut ws = WsClient::open(addr, &signal_path(&seat)).await;
    assert_eq!(ws.recv_json().await["type"], "ice");

    ws.send_text("this is not json").await;
    let err = ws.recv_json().await;
    assert_eq!(err["type"], "error");
    assert!(err["message"].as_str().unwrap().contains("malformed"));

    ws.send_text(r#"{"type":"offer","sdp":"v=0 still alive"}"#)
        .await;
    assert_eq!(
        next_signal(&mut inbound).await,
        SignalIn::Offer {
            seat: seat.clone(),
            sdp: "v=0 still alive".into()
        }
    );
    ws.send_text(
        r#"{"type":"candidate","candidate":"candidate:3 1 udp 3 h 9 typ host","sdpIndex":1}"#,
    )
    .await;
    match next_signal(&mut inbound).await {
        SignalIn::Candidate {
            sdp_mid, sdp_index, ..
        } => {
            assert_eq!(sdp_mid, None);
            assert_eq!(sdp_index, Some(1), "the sdpIndex alias must parse");
        }
        other => panic!("expected a candidate, got {other:?}"),
    }
    ws.send_text(r#"{"type":"close"}"#).await;
    assert_eq!(next_signal(&mut inbound).await, SignalIn::Close { seat });
}

#[test]
fn parse_ice_is_lenient_because_the_blob_is_operator_supplied() {
    let good = parse_ice(r#"[{"urls":["stun:a"]},{"urls":["turn:b"],"username":"u"}]"#);
    assert_eq!(
        good.as_array().unwrap().len(),
        2,
        "a valid list is relayed verbatim"
    );
    assert_eq!(parse_ice("{not json"), serde_json::Value::Array(vec![]));
    assert_eq!(parse_ice(""), serde_json::Value::Array(vec![]));
}

#[test]
fn server_frames_match_the_documented_shapes() {
    assert_eq!(
        serde_json::to_string(&ServerFrame::from(SignalOut::IceServers {
            config: r#"[{"urls":["stun:a"]}]"#.into()
        }))
        .unwrap(),
        r#"{"type":"ice","config":[{"urls":["stun:a"]}]}"#
    );
    assert_eq!(
        serde_json::to_string(&ServerFrame::from(SignalOut::Answer { sdp: "v=0".into() })).unwrap(),
        r#"{"type":"answer","sdp":"v=0"}"#
    );
    assert_eq!(
        serde_json::to_string(&ServerFrame::from(SignalOut::Candidate {
            candidate: "c".into(),
            sdp_mid: None,
            sdp_index: None,
        }))
        .unwrap(),
        r#"{"type":"candidate","candidate":"c"}"#
    );
    assert_eq!(
        serde_json::to_string(&ServerFrame::from(SignalOut::Error {
            message: "boom".into()
        }))
        .unwrap(),
        r#"{"type":"error","message":"boom"}"#
    );
}
