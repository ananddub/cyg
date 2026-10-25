//! `/ws/dashboard` — the overview page's live view.

use std::time::Duration;

use auto_route::controller;
use axum::extract::State;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::response::Response;
use engine_dashboard::WsFrame;
use futures::{SinkExt, StreamExt};
use tokio::sync::mpsc;

use crate::ApiState;
use crate::services::dashboard::frames;

struct DashboardWsController;

#[controller("/ws")]
impl DashboardWsController {
    fn new() -> Self {
        Self
    }

    #[get("/dashboard")]
    async fn upgrade(&self, ws: WebSocketUpgrade) -> Response {
        upgrade(ws, State(crate::current_state())).await
    }
}

/// How long a frame waits for a slow client before it is dropped.
const SEND_GRACE: Duration = Duration::from_millis(250);

pub(crate) async fn upgrade(ws: WebSocketUpgrade, State(state): State<ApiState>) -> Response {
    ws.on_upgrade(move |socket| serve(socket, state))
}

async fn serve(socket: WebSocket, state: ApiState) {
    let (mut sink, mut stream) = socket.split();

    if sink
        .send(text_frame(&frames::hello_frame(&state)))
        .await
        .is_err()
    {
        return;
    }

    // The spawned writer owns the sink so an in-flight frame is never cancelled.
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
    ticker.tick().await;

    loop {
        tokio::select! {
            _ = ticker.tick() => {
                match tokio::time::timeout(SEND_GRACE, tx.send(frames::snapshot_frame(&state))).await {
                    Ok(Ok(())) => {}
                    Ok(Err(_)) => break,
                    Err(_) => dropped += 1,
                }
            }
            msg = stream.next() => match msg {
                None | Some(Err(_)) | Some(Ok(Message::Close(_))) => break,
                Some(Ok(_)) => {}
            },
        }
    }

    drop(tx);
    writer.abort();
    if dropped > 0 {
        tracing::info!(
            "[dashboard-ws] socket closed after dropping {dropped} frame(s) for a client that fell behind"
        );
    }
}

fn text_frame(frame: &WsFrame) -> Message {
    Message::Text(
        serde_json::to_string(frame)
            .expect("dashboard frame serialises")
            .into(),
    )
}

#[cfg(test)]
mod tests;
