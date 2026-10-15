use super::*;
use super::raw_ws::WsClient;
use serde_json::json;

#[tokio::test]
async fn a_reconnect_supersedes_the_old_socket_without_losing_its_route() {
    let (state, mut inbound) = test_state();
    let addr = spawn_plane(&state).await;
    let seat = mint_seat(&state);
    let path = signal_path(&seat);

    let mut first = WsClient::open(addr, &path).await;
    assert_eq!(first.recv_json().await["type"], "ice");
    let mut second = WsClient::open(addr, &path).await;
    assert_eq!(second.recv_json().await["type"], "ice");
    // One route per seat: the newer attach owns it.
    assert_eq!(state.signals.peers(), 1);

    // The old socket tears down — its SeatReplies::Drop runs detach with a
    // stale generation and must NOT delete the second socket's route.
    first.send_text(r#"{"type":"close"}"#).await;
    assert_eq!(next_signal(&mut inbound).await, SignalIn::Close { seat: seat.clone() });
    assert!(
        state.signals.publish(&seat.id, SignalOut::Answer { sdp: "still here".into() }),
        "the stale socket's teardown deleted the live route"
    );
    assert_eq!(second.recv_json().await, json!({ "type": "answer", "sdp": "still here" }));

    // And the replacement socket is owed its own single Close.
    second.send_text(r#"{"type":"close"}"#).await;
    assert_eq!(next_signal(&mut inbound).await, SignalIn::Close { seat: seat.clone() });
    expect_quiet(&mut inbound).await;
    wait_peers(&state.signals, 0).await;
}

#[tokio::test]
async fn a_dropped_socket_still_teaches_the_data_plane_once() {
    // No `{"type":"close"}`, no Close frame: just the TCP connection going
    // away. The guard, not the loop, owns the notice.
    let (state, mut inbound) = test_state();
    let addr = spawn_plane(&state).await;
    let seat = mint_seat(&state);
    let mut ws = WsClient::open(addr, &signal_path(&seat)).await;
    assert_eq!(ws.recv_json().await["type"], "ice");
    drop(ws);
    assert_eq!(next_signal(&mut inbound).await, SignalIn::Close { seat });
    expect_quiet(&mut inbound).await;
    wait_peers(&state.signals, 0).await;
}
