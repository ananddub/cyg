//! # Engine HTTP
//!
//! A protocol-pluggable HTTP endpoint framework on raw `mio` — no async
//! runtime, no hyper/axum. Endpoints are defined once against a transport-
//! agnostic [`Api`] command interface; the wire version (`HttpVersion::Http1`
//! today, `Http3` later) is selected only when calling `serve`, so switching
//! protocols never touches endpoint code.
//!
//! - `SO_REUSEPORT` listener, thread-per-core compatible (spawn N `Api::serve`
//!   workers on one port, or drive inline in the tick loop via `build_backend`)
//! - Incremental HTTP/1.1 parsing with `Content-Length` bodies and keep-alive
//! - [`Router`](router::Router) with `:param` segments and `*` wildcards
//! - Handlers are plain `Fn(&Request) -> Response` closures; capture engine
//!   state (telemetry `Arc`s, room handles) directly

pub mod api;
pub mod backend;
pub mod parse;
pub mod router;
pub mod server;
pub mod types;
pub mod writer;

pub use api::Api;
pub use backend::{HttpBackend, HttpVersion, create_backend, serve};
pub use parse::RequestDecoder;
pub use router::Router;
pub use server::{Http1Backend, ServerHandle};
pub use types::{Headers, Method, Request, Response, json_escape};
