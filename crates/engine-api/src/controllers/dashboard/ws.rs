use std::time::Duration;

use auto_route::controller;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::State;
use axum::response::{IntoResponse, Response};
use engine_dashboard::WsFrame;
use futures::{SinkExt, StreamExt};
use serde::Deserialize;
use tokio::sync::mpsc;
use uuid::Uuid;

use crate::ApiState;
use crate::app::params::Params;
use crate::controllers::dashboard::api::guard;
use crate::infra::realtime::{LiveTopic, Subscription};
use crate::services::dashboard::frames;

#[derive(Debug, Default, Deserialize)]
struct DashboardScope {
    #[serde(default)]
    game_id: Option<Uuid>,
}

struct DashboardWsController;

#[controller("/ws")]
impl DashboardWsController {
    fn new() -> Self {
        Self
    }

    #[get("/dashboard")]
    async fn upgrade(
        &self,
        ws: WebSocketUpgrade,
        Params(scope): Params<DashboardScope>,
    ) -> Response {
        let state = crate::current_state();
        if let Err(err) = guard(&state, scope.game_id).await {
            return err.into_response();
        }
        upgrade(ws, State(state), scope.game_id).await
    }
}

const SEND_GRACE: Duration = Duration::from_millis(250);

pub(crate) async fn upgrade(
    ws: WebSocketUpgrade,
    State(state): State<ApiState>,
    game: Option<Uuid>,
) -> Response {
    ws.on_upgrade(move |socket| serve(socket, state, game))
}

async fn serve(socket: WebSocket, state: ApiState, game: Option<Uuid>) {
    let (mut sink, mut stream) = socket.split();

    if sink
        .send(text_frame(&frames::hello_frame(&state)))
        .await
        .is_err()
    {
        return;
    }

    let (tx, mut rx) = mpsc::channel::<WsFrame>(1);
    let writer = tokio::spawn(async move {
        while let Some(frame) = rx.recv().await {
            if sink.send(text_frame(&frame)).await.is_err() {
                return;
            }
        }
    });

    let feeds = match game {
        Some(game_id) => {
            let mut feeds = Vec::with_capacity(LiveTopic::ALL.len());
            for topic in LiveTopic::ALL {
                match state.realtime.subscribe(*topic, game_id).await {
                    Ok(subscription) => feeds.push(spawn_feed(subscription, tx.clone())),
                    Err(err) => {
                        tracing::warn!(
                            "[dashboard-ws] the {} feed is unavailable: {err}",
                            topic.name()
                        );
                    }
                }
            }
            feeds
        }
        None => Vec::new(),
    };

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
    for feed in feeds {
        feed.abort();
    }
    writer.abort();
    if dropped > 0 {
        tracing::info!(
            "[dashboard-ws] socket closed after dropping {dropped} frame(s) for a client that fell behind"
        );
    }
}

fn spawn_feed(mut subscription: Subscription, tx: mpsc::Sender<WsFrame>) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        while let Some(event) = subscription.recv().await {
            let frame = WsFrame::Live {
                topic: event.topic.to_string(),
                action: event.action.to_string(),
                record: event.record,
                seq: event.seq,
            };
            match tx.try_send(frame) {
                Ok(()) | Err(mpsc::error::TrySendError::Full(_)) => {}
                Err(mpsc::error::TrySendError::Closed(_)) => return,
            }
        }
    })
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
