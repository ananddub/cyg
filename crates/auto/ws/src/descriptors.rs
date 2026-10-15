use std::collections::HashMap;
use std::sync::Arc;

use auto_di::{BoxFuture, Container, DiError};
use serde_json::Value;

use crate::{BoxFuture as SocketFuture, Conn, Msg, WsError};

/// One `#[on]` handler: the resolved socket object plus the method to call.
pub type EventHandler =
    Arc<dyn Fn(Conn, Msg) -> SocketFuture<Result<Option<Value>, WsError>> + Send + Sync>;

/// One `#[connect]` hook, run in declaration order before the read loop.
pub type ConnectHook = Arc<dyn Fn(Conn) -> SocketFuture<()> + Send + Sync>;

/// One `#[disconnect]` hook, run in declaration order after the read loop.
pub type DisconnectHook = Arc<dyn Fn(Conn) -> SocketFuture<()> + Send + Sync>;

/// Everything one socket path declares: its events and its lifecycle hooks.
#[derive(Default)]
pub struct SocketRegistrar {
    pub events: HashMap<&'static str, EventHandler>,
    pub connects: Vec<ConnectHook>,
    pub disconnects: Vec<DisconnectHook>,
}

impl SocketRegistrar {
    /// Fold a second declaration of the same path in. Later events win, which
    /// keeps a re-declared tag a deliberate override rather than a panic.
    pub fn merge(&mut self, other: SocketRegistrar) {
        self.events.extend(other.events);
        self.connects.extend(other.connects);
        self.disconnects.extend(other.disconnects);
    }
}

/// One `#[auto_ws]` impl, collected at compile time.
pub struct SocketDescriptor {
    pub path: &'static str,
    pub factory: for<'a> fn(&'a Container) -> BoxFuture<'a, Result<SocketRegistrar, DiError>>,
}

impl SocketDescriptor {
    pub const fn new(
        path: &'static str,
        factory: for<'a> fn(&'a Container) -> BoxFuture<'a, Result<SocketRegistrar, DiError>>,
    ) -> Self {
        Self { path, factory }
    }
}

inventory::collect!(SocketDescriptor);
