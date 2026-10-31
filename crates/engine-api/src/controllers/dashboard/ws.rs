use std::collections::BTreeSet;
use std::time::Duration;

use auto_route::controller;
use axum::extract::State;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::http::HeaderMap;
use axum::response::{IntoResponse, Response};
use engine_dashboard::WsFrame;
use futures::{SinkExt, StreamExt};
use serde::Deserialize;
use tokio::sync::broadcast::{self, error::RecvError};
use tokio::sync::mpsc;
use uuid::Uuid;

use crate::ApiState;
use crate::app::middleware::{bearer_token, console_token_claims};
use crate::app::params::Params;
use crate::controllers::dashboard::api::guard;
use crate::error::V2Error;
use crate::infra::realtime::{LiveTopic, Subscription};
use crate::services::dashboard::frames;

#[derive(Debug, Default, Deserialize)]
struct DashboardScope {
    #[serde(default)]
    game_id: Option<Uuid>,
    #[serde(default)]
    token: Option<String>,
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
        headers: HeaderMap,
        Params(scope): Params<DashboardScope>,
    ) -> Response {
        let state = crate::current_state();
        let token = scope.token.as_deref().or_else(|| bearer_token(&headers));
        let Some(token) = token else {
            return V2Error::unauthenticated("Console token required").into_response();
        };
        if let Err(err) = console_token_claims(token) {
            return err.into_response();
        }
        if let Err(err) = guard(&state, scope.game_id).await {
            return err.into_response();
        }
        upgrade(ws, State(state), scope.game_id).await
    }
}

const SEND_GRACE: Duration = Duration::from_millis(250);
/// Marks that land inside this window are folded into one wake-up per table, so
/// a burst of writes cannot turn into a burst of frames.
const TOUCH_QUIET: Duration = Duration::from_millis(200);

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

    let touches = state
        .realtime
        .touches()
        .map(|rx| spawn_touches(rx, tx.clone(), game.is_some()));

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
    let mut log_feed: Option<tokio::task::JoinHandle<()>> = None;
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
                Some(Ok(Message::Text(text))) => {
                    match log_request(&text) {
                        LogRequest::Subscribe(level) => {
                            if let Some(running) = log_feed.take() {
                                running.abort();
                            }
                            log_feed = Some(spawn_log_feed(tx.clone(), level));
                        }
                        LogRequest::Unsubscribe => {
                            if let Some(running) = log_feed.take() {
                                running.abort();
                            }
                        }
                        LogRequest::None => {}
                    }
                }
                Some(Ok(_)) => {}
            },
        }
    }

    drop(tx);
    for feed in feeds {
        feed.abort();
    }
    if let Some(feed) = touches {
        feed.abort();
    }
    if let Some(running) = log_feed.take() {
        running.abort();
    }
    writer.abort();
    if dropped > 0 {
        tracing::info!(
            "[dashboard-ws] socket closed after dropping {dropped} frame(s) for a client that fell behind"
        );
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum LogLevel {
    Error = 0,
    Warn = 1,
    Info = 2,
    Debug = 3,
    Trace = 4,
}

fn level_rank(name: &str) -> Option<LogLevel> {
    match name.to_ascii_lowercase().as_str() {
        "error" => Some(LogLevel::Error),
        "warn" | "warning" => Some(LogLevel::Warn),
        "info" => Some(LogLevel::Info),
        "debug" => Some(LogLevel::Debug),
        "trace" => Some(LogLevel::Trace),
        _ => None,
    }
}

enum LogRequest {
    Subscribe(LogLevel),
    Unsubscribe,
    None,
}

fn log_request(text: &axum::extract::ws::Utf8Bytes) -> LogRequest {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(text.as_str()) else {
        return LogRequest::None;
    };
    match value.get("type").and_then(serde_json::Value::as_str) {
        Some("logs.subscribe") => {
            let level = value
                .get("level")
                .and_then(serde_json::Value::as_str)
                .and_then(level_rank)
                .unwrap_or(LogLevel::Info);
            LogRequest::Subscribe(level)
        }
        Some("logs.unsubscribe") => LogRequest::Unsubscribe,
        _ => LogRequest::None,
    }
}

/// Mirrors the log feed to one socket. A client that stops reading is skipped
/// past (the channel lags) rather than slow: the socket keeps its telemetry.
fn spawn_log_feed(tx: mpsc::Sender<WsFrame>, max: LogLevel) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut feed = crate::infra::logfeed::subscribe();
        loop {
            let Ok(first) = feed.recv().await else {
                continue;
            };
            let mut lines = Vec::with_capacity(16);
            if level_rank(&first.level).is_some_and(|rank| rank <= max) {
                lines.push(first);
            }
            while lines.len() < 50 {
                match feed.try_recv() {
                    Ok(next) => {
                        if level_rank(&next.level).is_some_and(|rank| rank <= max) {
                            lines.push(next);
                        }
                    }
                    Err(_) => break,
                }
            }
            if lines.is_empty() {
                continue;
            }
            let frame = WsFrame::Logs { lines };
            match tokio::time::timeout(SEND_GRACE, tx.send(frame)).await {
                Ok(Ok(())) | Err(_) => {}
                Ok(Err(_)) => return,
            }
        }
    })
}

/// Turns the store's bare write marks into frames. A scoped socket already gets
/// row-level frames for the account-plane tables, so their marks are skipped
/// there; every other table has no such feed, and its mark is the only sign the
/// page gets that what it shows has moved.
fn spawn_touches(
    mut rx: broadcast::Receiver<String>,
    tx: mpsc::Sender<WsFrame>,
    scoped: bool,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut seq = 0u64;
        loop {
            let first = match rx.recv().await {
                Ok(table) => table,
                Err(RecvError::Lagged(_)) => continue,
                Err(RecvError::Closed) => return,
            };
            let mut changed = BTreeSet::new();
            if !(scoped && LiveTopic::covers_table(&first)) {
                changed.insert(first);
            }
            let quiet = tokio::time::sleep(TOUCH_QUIET);
            tokio::pin!(quiet);
            loop {
                tokio::select! {
                    _ = &mut quiet => break,
                    next = rx.recv() => match next {
                        Ok(table) => {
                            if !(scoped && LiveTopic::covers_table(&table)) {
                                changed.insert(table);
                            }
                        }
                        Err(RecvError::Lagged(_)) => {}
                        Err(RecvError::Closed) => break,
                    },
                }
            }
            for table in changed {
                seq += 1;
                let frame = WsFrame::Live {
                    topic: table,
                    action: "touch".to_string(),
                    record: String::new(),
                    seq,
                };
                match tx.try_send(frame) {
                    Ok(()) | Err(mpsc::error::TrySendError::Full(_)) => {}
                    Err(mpsc::error::TrySendError::Closed(_)) => return,
                }
            }
        }
    })
}

fn spawn_feed(
    mut subscription: Subscription,
    tx: mpsc::Sender<WsFrame>,
) -> tokio::task::JoinHandle<()> {
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
