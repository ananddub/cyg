use std::collections::HashMap;
use std::path::Path;
use std::time::Duration;

use engine_bus::Bus;
use engine_config::{ControlConfig, GameConfig};
use futures::{SinkExt, StreamExt};
use tokio::net::TcpListener;
use tokio_tungstenite::tungstenite::Message as Raw;

use engine_api::{ApiState, serve};

async fn state(port: u16) -> ApiState {
    let game = GameConfig {
        name: "tank-3d".into(),
        ..GameConfig::default()
    };
    let control = ControlConfig {
        api_port: 0,
        ..ControlConfig::default()
    };
    ApiState::new(Path::new("."), &game, &control, Bus::new(16, 16), port).await
}

async fn plane() -> (u16, String, tokio::task::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("ephemeral bind");
    let port = listener.local_addr().expect("bound addr").port();
    let (session, _) =
        engine_api::infra::jwt::mint_token_pair(uuid::Uuid::new_v4(), "player", HashMap::new());
    let server = tokio::spawn(serve(listener, state(port).await, std::future::pending()));
    (port, session, server)
}

pub(crate) async fn next_json<S>(ws: &mut tokio_tungstenite::WebSocketStream<S>) -> serde_json::Value
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
{
    loop {
        let msg = tokio::time::timeout(Duration::from_secs(10), ws.next())
            .await
            .expect("a frame within 10 s")
            .expect("socket stays open")
            .expect("tungstenite");
        if let Raw::Text(text) = msg {
            return serde_json::from_str(text.as_str()).expect("frame is JSON");
        }
    }
}

#[tokio::test]
async fn a_socket_answers_ping_on_the_cid_and_shapes_message_errors() {
    let (port, token, server) = plane().await;
    let url = format!("ws://127.0.0.1:{port}/ws?token={token}");
    let (mut ws, _) = tokio_tungstenite::connect_async(&url)
        .await
        .expect("socket connects with a session token");

    ws.send(Raw::Text(r#"{"cid":"1","type":"ping"}"#.into()))
        .await
        .expect("ping sent");
    let pong = next_json(&mut ws).await;
    assert_eq!(
        pong["cid"].as_str(),
        Some("1"),
        "the reply carries the request cid"
    );
    assert_eq!(pong["type"].as_str(), Some("pong"));

    ws.send(Raw::Text(r#"{"cid":"2","type":"lobby_join"}"#.into()))
        .await
        .expect("unknown tag sent");
    let refused = next_json(&mut ws).await;
    assert_eq!(refused["type"].as_str(), Some("error"));
    assert!(
        refused.get("code").is_none(),
        "an error frame carries no numeric code: {refused}"
    );
    assert!(
        refused["cid"].is_null(),
        "an undecodable frame has no cid to answer on"
    );
    assert!(
        refused["message"]
            .as_str()
            .unwrap_or_default()
            .starts_with("cannot parse envelope:"),
        "expected the parse wording, got {refused}"
    );

    server.abort();
}

#[tokio::test]
async fn an_upgrade_without_a_session_token_never_opens_a_socket() {
    let (port, _token, server) = plane().await;
    let url = format!("ws://127.0.0.1:{port}/ws");

    assert!(
        tokio_tungstenite::connect_async(&url).await.is_err(),
        "a tokened protocol must refuse a tokenless upgrade"
    );

    server.abort();
}

#[tokio::test]
async fn closing_the_socket_leaves_the_session_behind() {
    let (port, _token, server) = plane().await;
    let uid = uuid::Uuid::new_v4();
    let (session, _) = engine_api::infra::jwt::mint_token_pair(uid, "player", HashMap::new());
    let url = format!("ws://127.0.0.1:{port}/ws?token={session}");
    let (mut ws, _) = tokio_tungstenite::connect_async(&url)
        .await
        .expect("socket connects");

    ws.send(Raw::Text(r#"{"cid":"1","type":"ping"}"#.into()))
        .await
        .expect("ping sent");
    let _ = next_json(&mut ws).await;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    while !engine_api::infra::ws::session::is_online(uid) && tokio::time::Instant::now() < deadline
    {
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert!(
        engine_api::infra::ws::session::is_online(uid),
        "connected session is registered"
    );

    ws.close(None).await.expect("close sent");

    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    while engine_api::infra::ws::session::is_online(uid) && tokio::time::Instant::now() < deadline {
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert!(
        !engine_api::infra::ws::session::is_online(uid),
        "teardown must unregister the session"
    );

    server.abort();
}
