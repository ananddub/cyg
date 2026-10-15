//! `/ws/dashboard`: one JSON [`engine_dashboard::WsFrame`] per second.
//!
//! Owned by the dashboard workstream. `lib.rs` merges [`router`] into the plane.
//!
//! The socket speaks the same objects as the REST routes — `hello` on open,
//! then `snapshot` once a second, all built through the [`crate::dashboard`]
//! seam — so a page load paints immediately and a poll can never disagree with
//! a push. Unauthenticated, like every other route on this plane: the
//! dashboard is same-origin and the network is the boundary.
//!
//! A slow client must never back-pressure the plane: the tick loop hands each
//! frame to a dedicated writer through a one-slot channel and waits at most
//! [`SEND_GRACE`] for it to be taken. A frame that does not make it is dropped
//! and counted; only the newest sample matters to a dashboard.

use std::time::Duration;

use axum::Router;
use axum::extract::State;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::response::Response;
use axum::routing::get;
use engine_dashboard::WsFrame;
use futures::{SinkExt, StreamExt};
use tokio::sync::mpsc;

use crate::ControlState;

/// How long a snapshot may wait for the writer before it is dropped. The writer
/// itself is never timed out — cancelling a half-written socket frame is not
/// safe — so a stalled client stalls only this one socket's writer task, never
/// the tick loop.
const SEND_GRACE: Duration = Duration::from_millis(250);

/// `pub fn router(&ControlState) -> Router<ControlState>` — frozen entry point.
pub fn router(state: &ControlState) -> Router<ControlState> {
    let _ = state;
    Router::new().route("/ws/dashboard", get(upgrade))
}

async fn upgrade(ws: WebSocketUpgrade, State(state): State<ControlState>) -> Response {
    ws.on_upgrade(move |socket| serve(socket, state))
}

async fn serve(socket: WebSocket, state: ControlState) {
    let (mut sink, mut stream) = socket.split();

    if sink.send(text_frame(&hello_frame(&state))).await.is_err() {
        return; // the client is gone before its first paint
    }

    // The writer owns the sink for the socket's whole life, so nothing ever
    // cancels an in-flight frame; it exits on the first send error and the
    // closed channel tells the loop below.
    let (tx, mut rx) = mpsc::channel::<WsFrame>(1);
    let writer = tokio::spawn(async move {
        while let Some(frame) = rx.recv().await {
            if sink.send(text_frame(&frame)).await.is_err() {
                return;
            }
        }
    });

    let mut dropped = 0usize;
    let mut ticker = tokio::time::interval(Duration::from_secs(1));
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    ticker.tick().await; // the first tick is instant and `hello` already covered it

    loop {
        tokio::select! {
            _ = ticker.tick() => {
                match tokio::time::timeout(SEND_GRACE, tx.send(snapshot_frame(&state))).await {
                    Ok(Ok(())) => {}
                    Ok(Err(_)) => break, // writer gave up: socket error or close
                    Err(_) => dropped += 1,
                }
            }
            msg = stream.next() => match msg {
                // Reading is how a client-initiated close is noticed at all;
                // the dashboard never sends anything worth acting on.
                None | Some(Err(_)) | Some(Ok(Message::Close(_))) => break,
                Some(Ok(_)) => {}
            },
        }
    }

    drop(tx);
    writer.abort();
    if dropped > 0 {
        eprintln!("[dashboard-ws] socket closed after dropping {dropped} frame(s) for a client that fell behind");
    }
}

fn hello_frame(state: &ControlState) -> WsFrame {
    WsFrame::Hello {
        overview: Box::new(crate::dashboard::build_overview(state)),
        ticks: Box::new(crate::dashboard::build_ticks()),
        protocol: engine_dashboard::WS_PROTOCOL,
    }
}

fn snapshot_frame(state: &ControlState) -> WsFrame {
    WsFrame::Snapshot {
        overview: Box::new(crate::dashboard::build_overview(state)),
        ticks: Box::new(crate::dashboard::build_ticks()),
    }
}

fn text_frame(frame: &WsFrame) -> Message {
    Message::Text(serde_json::to_string(frame).expect("dashboard frame serialises").into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::OnceReceiver;
    use crate::rate_limit::IpRateLimiter;
    use crate::seats::SeatRegistry;
    use crate::signaling::SignalHub;
    use crate::static_routes::StaticServer;
    use engine_bus::Bus;
    use engine_config::{ControlConfig, GameConfig};
    use std::collections::BTreeSet;
    use std::path::Path;
    use std::sync::Arc;

    /// Same hand-built plane as `status::tests`: every `ControlState` field is
    /// public so telemetry is testable without a sim thread.
    fn state() -> ControlState {
        let game = GameConfig { name: "tank-3d".into(), ..GameConfig::default() };
        let control = ControlConfig { api_port: 0, ..ControlConfig::default() };
        let bus = Bus::new(16, 16);
        ControlState {
            statics: Arc::new(StaticServer::new(Path::new("."), &game)),
            game: Arc::new(game),
            control: Arc::new(control),
            signals: SignalHub::new("[]".into()),
            seats: Arc::new(std::sync::Mutex::new(SeatRegistry::new("test-secret"))),
            limiter: Arc::new(std::sync::Mutex::new(IpRateLimiter::new(20, 40))),
            port: 8124,
            status_rx: Arc::new(OnceReceiver::new(bus.status_rx.clone())),
            latest_status: Arc::new(std::sync::Mutex::new(None)),
            bus,
            started: std::time::Instant::now(),
        }
    }

    fn overview_keys(frame: &serde_json::Value) -> BTreeSet<String> {
        frame["overview"].as_object().expect("overview is an object").keys().cloned().collect()
    }

    #[test]
    fn hello_then_snapshot_carry_the_same_overview_keys() {
        let s = state();
        let hello = serde_json::to_value(hello_frame(&s)).unwrap();
        let snapshot = serde_json::to_value(snapshot_frame(&s)).unwrap();
        assert_eq!(hello["type"].as_str(), Some("hello"));
        assert_eq!(snapshot["type"].as_str(), Some("snapshot"));
        assert_eq!(hello["protocol"].as_u64(), Some(u64::from(engine_dashboard::WS_PROTOCOL)));
        assert!(snapshot.get("protocol").is_none(), "only hello negotiates the protocol");
        assert_eq!(overview_keys(&hello), overview_keys(&snapshot));
        assert!(snapshot["ticks"].is_object(), "a snapshot must be enough to redraw the graphs");
    }

    /// Even a full 300-point history plus every table must stay a modest frame:
    /// one socket per open dashboard tab, once a second.
    #[test]
    fn a_frame_is_bounded_in_size() {
        let json = serde_json::to_string(&snapshot_frame(&state())).unwrap();
        assert!(json.len() < 128 * 1024, "snapshot frame is {} bytes", json.len());
    }

    /// `spawn` builds its runtime with `block_on` calls, which panic inside an
    /// async context, so the plane is bootstrapped on a blocking thread.
    #[tokio::test]
    async fn a_live_socket_receives_hello_then_a_snapshot() {
        use tokio_tungstenite::connect_async;

        let plane = tokio::task::spawn_blocking(|| {
            let game = GameConfig { name: "tank-3d".into(), ..GameConfig::default() };
            let control = ControlConfig { api_port: 0, ..ControlConfig::default() };
            crate::spawn(Path::new("."), &game, &control, Bus::new(16, 16))
        })
        .await
        .expect("plane thread")
        .expect("plane on a kernel-chosen port");
        let url = format!("ws://127.0.0.1:{}/ws/dashboard", plane.port);

        let (mut ws, _) = connect_async(&url).await.expect("dashboard socket connects");
        let hello = next_json(&mut ws).await;
        assert_eq!(hello["type"].as_str(), Some("hello"));
        let snapshot = next_json(&mut ws).await;
        assert_eq!(snapshot["type"].as_str(), Some("snapshot"));
        assert_eq!(overview_keys(&hello), overview_keys(&snapshot));

        // A runtime may not be dropped from an async context either.
        tokio::task::spawn_blocking(move || plane.shutdown())
            .await
            .expect("shutdown thread");
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
}
