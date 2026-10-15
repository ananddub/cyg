//! `auto_ws` registers axum websockets and their event handlers through
//! compile-time inventory, resolving every socket object with
//! [`auto-di`](https://crates.io/crates/auto-di).
//!
//! Where `auto_route` turns an impl into HTTP bindings, `auto_ws` turns one
//! into a socket: `#[auto_ws("/ws")]` names the axum path, `#[on("event")]`
//! binds one inbound event tag, and `#[connect]` / `#[disconnect]` are the
//! per-connection lifecycle hooks.
//!
//! ```rust,no_run
//! use std::sync::Arc;
//!
//! use auto_di::singleton;
//! use auto_ws::{Conn, Msg, WsError, auto_ws};
//!
//! struct ChatService;
//!
//! #[singleton]
//! impl ChatService {
//!     fn new() -> Self {
//!         Self
//!     }
//! }
//!
//! struct ChatSocket {
//!     service: Arc<ChatService>,
//! }
//!
//! // `#[auto_ws]` registers the socket with `auto-di` itself, like
//! // `auto_route`'s `#[controller]`.
//! #[auto_ws("/ws")]
//! impl ChatSocket {
//!     fn new(service: Arc<ChatService>) -> Self {
//!         Self { service }
//!     }
//!
//!     #[connect]
//!     async fn connect(&self, _conn: &Conn) {}
//!
//!     #[on("ping")]
//!     async fn ping(&self, _conn: &Conn, msg: &Msg) -> Result<Option<serde_json::Value>, WsError> {
//!         let _ = self.service;
//!         let _ = msg;
//!         Ok(Some(serde_json::json!({ "type": "pong" })))
//!     }
//! }
//! ```
//!
//! Mount the sockets on the router that already owns your other routes:
//!
//! ```rust,no_run
//! # use axum::{Router, routing::get};
//! # use axum::extract::ws::WebSocketUpgrade;
//! # struct MyProtocol;
//! # impl auto_ws::Protocol for MyProtocol {
//! #     fn authenticate(
//! #         _parts: &axum::http::request::Parts,
//! #     ) -> Result<auto_ws::Identity, axum::response::Response> {
//! #         unimplemented!()
//! #     }
//! #     fn decode(_frame: &str) -> Result<auto_ws::Msg, auto_ws::WsError> {
//! #         unimplemented!()
//! #     }
//! #     fn encode_error(_cid: Option<&str>, _error: &auto_ws::WsError) -> String {
//! #         unimplemented!()
//! #     }
//! # }
//! # async fn mount(container: &auto_di::Container, router: Router) -> Router {
//! auto_ws::mount::<MyProtocol>(container, router).await.expect("sockets resolve")
//! # }
//! ```
//!
//! # How a connection runs
//!
//! 1. axum hands the upgrade request to the mounted route; [`Protocol::authenticate`]
//!    decides whether it becomes a socket and what [`Identity`] it carries
//!    (and can stash app data, e.g. the plane state, on that identity).
//! 2. the `#[connect]` hooks of the path run in declaration order — that is
//!    where a session is built and registered;
//! 3. each text frame is decoded by [`Protocol::decode`] into a [`Msg`] and
//!    routed by its event tag to the matching `#[on]` handler;
//! 4. handler results (`Result<Option<T>, E>`) are written back with the
//!    request's `cid`; errors go through [`Protocol::encode_error`];
//! 5. protocol pings are answered by the runtime, and when the socket closes
//!    the `#[disconnect]` hooks run in declaration order.
//!
//! The writer is one task-owned channel: every reply — handler result, plane
//! push or error — goes through [`Conn`], so writes stay ordered.

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

/// A boxed `Send` future, the shape every registered hook returns.
pub type BoxFuture<T> =
    std::pin::Pin<std::boxed::Box<dyn std::future::Future<Output = T> + Send + 'static>>;

#[doc(hidden)]
pub mod __private {
    pub use auto_di;
    pub use inventory;
    pub use serde_json;
}
