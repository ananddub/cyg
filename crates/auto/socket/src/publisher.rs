use crate::{
    LiveIdentity,
    rooms::scoped,
    state::{LATEST_CHANNELS, SOCKET_IO, STREAM_CHANNELS, STREAM_HISTORY, StreamHistory},
};
use serde::Serialize;
use socketioxide::extract::SocketRef;
use std::{collections::VecDeque, marker::PhantomData};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LiveStrategy {
    Publish,
    Sqlite,
    Latest,
    Stream { capacity: usize, replay: usize },
}
#[derive(Debug, thiserror::Error)]
pub enum PublishError {
    #[error("auto_socket has not been registered")]
    NotRegistered,
    #[error("failed to serialize live endpoint arguments: {0}")]
    Serialize(#[from] serde_json::Error),
    #[error("failed to publish socket event: {0}")]
    Broadcast(#[from] socketioxide::BroadcastError),
    #[error("live publisher requires endpoint room arguments")]
    MissingRoomArguments,
}
pub struct LivePublisher<T> {
    namespace: &'static str,
    endpoint: &'static str,
    _event: &'static str,
    room_args: Option<serde_json::Value>,
    strategy: LiveStrategy,
    identity: Option<LiveIdentity>,
    marker: PhantomData<fn() -> T>,
}
impl<T: Serialize> LivePublisher<T> {
    pub fn new(namespace: &'static str, endpoint: &'static str, event: &'static str) -> Self {
        Self {
            namespace,
            endpoint,
            _event: event,
            room_args: None,
            strategy: LiveStrategy::Publish,
            identity: None,
            marker: PhantomData,
        }
    }
    pub fn room<A: Serialize>(mut self, args: A) -> Result<Self, PublishError> {
        self.room_args = Some(serde_json::to_value(args)?);
        Ok(self)
    }
    pub fn strategy(mut self, strategy: LiveStrategy) -> Self {
        self.strategy = strategy;
        self
    }
    pub fn user(mut self, user_id: i64) -> Self {
        self.identity = Some(LiveIdentity {
            user_id,
            organization_id: None,
        });
        self
    }
    pub async fn publish(self, data: T) -> Result<(), PublishError> {
        let args = self.room_args.ok_or(PublishError::MissingRoomArguments)?;
        let message = serde_json::json!({"endpoint": self.endpoint, "args": args, "data": data});
        let room = scoped(self.identity.as_ref(), self.endpoint, &message["args"]);
        match self.strategy {
            LiveStrategy::Publish | LiveStrategy::Sqlite => {
                emit_room(self.namespace, room, message).await
            }
            LiveStrategy::Latest => publish_latest(self.namespace, room, message),
            LiveStrategy::Stream { capacity, replay } => {
                publish_stream(self.namespace, room, message, capacity, replay).await
            }
        }
    }
    pub fn emit(self, socket: &SocketRef, data: T) -> Result<(), socketioxide::SendError> {
        socket.emit(
            "live:update",
            &serde_json::json!({"endpoint": self.endpoint, "args": self.room_args, "data": data}),
        )
    }
    pub async fn broadcast(self, data: T) -> Result<(), PublishError> {
        let io = SOCKET_IO.get().ok_or(PublishError::NotRegistered)?;
        if let Some(namespace) = io.of(self.namespace) {
            namespace.emit("live:update", &serde_json::json!({"endpoint": self.endpoint, "args": self.room_args, "data": data})).await?;
        }
        Ok(())
    }
    pub fn endpoint(&self) -> &'static str {
        self.endpoint
    }
}
async fn emit_room(
    namespace: &'static str,
    room: String,
    message: serde_json::Value,
) -> Result<(), PublishError> {
    let io = SOCKET_IO.get().ok_or(PublishError::NotRegistered)?;
    let endpoint = message
        .get("endpoint")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("unknown");
    let args = message.get("args").cloned().unwrap_or_default();
    if let Some(socket_namespace) = io.of(namespace) {
        tracing::info!(namespace, room = %room, endpoint, args = ?args, "sending live update to socket room");
        let result = socket_namespace
            .to(room)
            .emit("live:update", &message)
            .await;
        match result {
            Ok(()) => {
                tracing::info!(namespace, endpoint, "live update sent to socket room");
            }
            Err(error) => {
                tracing::warn!(namespace, endpoint, error = %error, "failed to send live update to socket room");
                return Err(error.into());
            }
        }
    } else {
        tracing::warn!(
            namespace,
            endpoint,
            "live socket namespace is not registered"
        );
    }
    Ok(())
}
fn publish_latest(
    namespace: &'static str,
    room: String,
    message: serde_json::Value,
) -> Result<(), PublishError> {
    SOCKET_IO.get().ok_or(PublishError::NotRegistered)?;
    let key = format!("{namespace}:{room}");
    let mut channels = LATEST_CHANNELS
        .get_or_init(Default::default)
        .lock()
        .expect("latest live channel registry poisoned");
    if let Some(sender) = channels.get(&key) {
        sender.send_replace(message);
        return Ok(());
    }
    let (sender, mut receiver) = tokio::sync::watch::channel(message);
    channels.insert(key, sender);
    tokio::spawn(async move {
        loop {
            let value = receiver.borrow_and_update().clone();
            let _ = emit_room(namespace, room.clone(), value).await;
            if receiver.changed().await.is_err() {
                break;
            }
        }
    });
    Ok(())
}
async fn publish_stream(
    namespace: &'static str,
    room: String,
    message: serde_json::Value,
    capacity: usize,
    replay: usize,
) -> Result<(), PublishError> {
    SOCKET_IO.get().ok_or(PublishError::NotRegistered)?;
    let key = format!("{namespace}:{room}");
    if replay > 0 {
        let mut histories = STREAM_HISTORY
            .get_or_init(Default::default)
            .lock()
            .expect("stream history registry poisoned");
        let history = histories
            .entry(key.clone())
            .or_insert_with(|| StreamHistory {
                limit: replay,
                events: VecDeque::with_capacity(replay),
            });
        history.limit = replay;
        while history.events.len() >= history.limit {
            history.events.pop_front();
        }
        history.events.push_back(message.clone());
    }
    let sender = {
        let mut channels = STREAM_CHANNELS
            .get_or_init(Default::default)
            .lock()
            .expect("stream live channel registry poisoned");
        if let Some(sender) = channels.get(&key) {
            sender.clone()
        } else {
            let (sender, mut receiver) = tokio::sync::mpsc::channel(capacity.max(1));
            channels.insert(key, sender.clone());
            tokio::spawn(async move {
                while let Some(value) = receiver.recv().await {
                    let _ = emit_room(namespace, room.clone(), value).await;
                }
            });
            sender
        }
    };
    sender
        .send(message)
        .await
        .map_err(|_| PublishError::NotRegistered)
}
