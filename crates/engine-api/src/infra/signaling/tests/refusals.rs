use super::raw_ws::{connect_io, read_chunk, read_head};
use super::*;

async fn upgrade_get(addr: SocketAddr, path: &str) -> (u16, serde_json::Value) {
    let mut io = connect_io(addr, path).await;
    let head = read_head(&mut io).await.expect("no response head");
    let status: u16 = head
        .split_whitespace()
        .nth(1)
        .expect("status line")
        .parse()
        .expect("numeric status");
    let len = head
        .lines()
        .find_map(|l| {
            let (k, v) = l.split_once(':')?;
            if k.trim().eq_ignore_ascii_case("content-length") {
                v.trim().parse().ok()
            } else {
                None
            }
        })
        .unwrap_or(0usize);
    let mut body = vec![0u8; len];
    if len > 0 {
        read_chunk(&mut io, &mut body)
            .await
            .expect("body shorter than Content-Length");
    }
    let json =
        serde_json::from_slice(&body).unwrap_or_else(|e| panic!("error body is not JSON: {e}"));
    (status, json)
}

#[tokio::test]
async fn refuses_a_bad_or_missing_seat_token_before_the_upgrade() {
    let (state, mut inbound) = test_state();
    let addr = spawn_plane(&state).await;
    for path in [
        "/ws/signal",
        "/ws/signal?seat=not-a-signed-token",
        "/ws/signal?seat.",
    ] {
        let (status, body) = upgrade_get(addr, path).await;
        assert_eq!(status, 401, "{path} was not refused with 401");
        assert!(
            body["error"]["message"]
                .as_str()
                .unwrap_or("")
                .contains("seat"),
            "plane-wide error body missing: {body}"
        );
    }
    assert_eq!(
        state.signals.peers(),
        0,
        "a refused upgrade must never become a task"
    );
    expect_quiet(&mut inbound).await;
}
