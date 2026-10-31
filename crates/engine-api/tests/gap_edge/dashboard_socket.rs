use super::*;

pub async fn dashboard_socket(env: &Harness) {
    eprintln!("gap: dashboard websocket");
    let mut ws = PlainWs::connect(
        "gap-dashboard",
        format!("ws://127.0.0.1:{}/ws/dashboard", env.port),
    )
    .await;

    let hello = ws.next_json("the hello frame").await;
    assert_eq!(
        hello["type"], "hello",
        "the plane introduces itself: {hello}"
    );
    assert_eq!(hello["protocol"], 1, "the frozen dashboard protocol");
    assert_eq!(hello["overview"]["server"]["name"], GAME_NAME);
    assert!(hello["overview"]["seats"]["minted"].is_number());
    assert!(hello["ticks"]["points"].is_array(), "{hello}");

    let started = std::time::Instant::now();
    let snapshot = ws.next_json("the first snapshot").await;
    assert_eq!(
        snapshot["type"], "snapshot",
        "the tick feed follows: {snapshot}"
    );
    assert_eq!(snapshot["overview"]["server"]["name"], GAME_NAME);
    assert!(
        started.elapsed() >= Duration::from_millis(500),
        "hello arrives at once; a snapshot is a tick, not the handshake"
    );

    ws.shutdown().await;
}
pub async fn signal_socket(env: &Harness, seat_token: &str) {
    eprintln!("gap: signal websocket");

    for (label, query, want) in [
        (
            "no seat token at all",
            "",
            "a seat token from matchmake is required",
        ),
        (
            "a token nobody signed",
            "?seat=not.a.signed-token",
            "seat token is invalid or expired",
        ),
    ] {
        let resp = env
            .client
            .get(format!("{}/ws/signal{query}", env.base))
            .header("Upgrade", "websocket")
            .header("Connection", "Upgrade")
            .header("Sec-WebSocket-Key", "dGhlIHNhbXBsZSBub25jZQ==")
            .header("Sec-WebSocket-Version", "13")
            .send()
            .await
            .unwrap_or_else(|err| panic!("{label}: the upgrade request goes out: {err}"));
        let body = json_body(resp, StatusCode::UNAUTHORIZED, label).await;
        assert_eq!(body["error"]["message"], want, "{label}: {body}");
        assert_eq!(body["error"]["code"], 16, "{label}: 401 is UNAUTHENTICATED");
    }

    let mut ws = PlainWs::connect(
        "gap-signal",
        format!("ws://127.0.0.1:{}/ws/signal?seat={seat_token}", env.port),
    )
    .await;
    let ice = ws.next_json("the ice handout").await;
    assert_eq!(ice["type"], "ice", "the seat's first frame is ICE: {ice}");
    assert_eq!(
        ice["config"],
        json!([]),
        "the config crosses as a JSON value, and the default list is empty"
    );
    ws.shutdown().await;
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
pub async fn edge_static_dashboard_and_signal_surfaces_end_to_end() {
    let env = Harness::start().await;

    liveness(&env).await;
    root_redirect(&env).await;
    static_plane(&env).await;
    dashboard_assets(&env).await;

    let seat = matchmake_plane(&env).await;
    dashboard_api(&env, 5).await;
    dashboard_socket(&env).await;
    signal_socket(&env, &seat).await;

    tokio::time::sleep(Duration::from_millis(200)).await;
    env.stop().await;
    eprintln!(
        "gap: liveness, root redirect, static plane, dashboard assets, matchmake, \
         dashboard api, dashboard socket and the signal seat gate passed"
    );
}
