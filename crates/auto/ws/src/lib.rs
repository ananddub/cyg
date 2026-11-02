
pub use auto_ws_macros::{auto_ws, connect, disconnect, on};

mod conn;
mod descriptors;
mod protocol;
mod router;

pub use conn::{Conn, Extensions, Identity, Msg, WsError};
pub use descriptors::{
    ConnectHook, DisconnectHook, EventHandler, SocketDescriptor, SocketRegistrar,
};
pub use protocol::Protocol;
pub use router::{Reply, mount, reply};

pub type BoxFuture<T> =
    std::pin::Pin<std::boxed::Box<dyn std::future::Future<Output = T> + Send + 'static>>;

#[doc(hidden)]
pub mod __private {
    pub use auto_di;
    pub use inventory;
    pub use serde_json;
}
