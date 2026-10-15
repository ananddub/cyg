use std::path::Path;
use std::time::Duration;

use engine_bus::Bus;
use engine_config::{ControlConfig, GameConfig};
use futures::StreamExt;
use tokio::net::TcpListener;

use super::overview_keys;
use crate::{ApiState, serve};

#[tokio::test]
async fn a_live_socket_receives_hello_then_a_snapshot() {
    use tokio_tungstenite::connect_async;

    let game = GameConfig { name: "tank-3d".into(), ..GameConfig::default() };
    let control = ControlConfig { api_port: 0, ..ControlConfig::default() };
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("ephemeral bind");
    let port = listener.local_addr().expect("bound addr").port();
    let state = ApiState::new(Path::new("."), &game, &control, Bus::new(16, 16), port);
    let server = tokio::spawn(serve(listener, state, std::future::pending()));

    let url = format!("ws://127.0.0.1:{port}/ws/dashboard");
    let (mut ws, _) = connect_async(&url).await.expect("dashboard socket connects");
    let hello = next_json(&mut ws).await;
    assert_eq!(hello["type"].as_str(), Some("hello"));
    let snapshot = next_json(&mut ws).await;
    assert_eq!(snapshot["type"].as_str(), Some("snapshot"));
    assert_eq!(overview_keys(&hello), overview_keys(&snapshot));

    server.abort();
}

async fn next_json<S>(ws: &mut tokio_tungstenite::WebSocketStream<S>) -> serde_json::Value
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
{
    use tokio_tungstenite::tungstenite::Message as Raw;
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
