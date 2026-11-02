use crate::{LiveIdentity, rooms::scoped, state::SOCKET_IO};
use serde::Serialize;
use socketioxide::extract::SocketRef;
use std::marker::PhantomData;

mod emit;

use emit::{emit_room, publish_latest, publish_stream};

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
