use std::collections::HashMap;
use std::sync::Arc;

use auto_di::{BoxFuture, Container, DiError};
use serde_json::Value;

use crate::{BoxFuture as SocketFuture, Conn, Msg, WsError};

pub type EventHandler =
    Arc<dyn Fn(Conn, Msg) -> SocketFuture<Result<Option<Value>, WsError>> + Send + Sync>;

pub type ConnectHook = Arc<dyn Fn(Conn) -> SocketFuture<()> + Send + Sync>;

pub type DisconnectHook = Arc<dyn Fn(Conn) -> SocketFuture<()> + Send + Sync>;

#[derive(Default)]
pub struct SocketRegistrar {
    pub events: HashMap<&'static str, EventHandler>,
    pub connects: Vec<ConnectHook>,
    pub disconnects: Vec<DisconnectHook>,
}

impl SocketRegistrar {

    pub fn merge(&mut self, other: SocketRegistrar) {
        self.events.extend(other.events);
        self.connects.extend(other.connects);
        self.disconnects.extend(other.disconnects);
    }
}

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
