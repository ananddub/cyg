//! # Engine WebSocket
//!
//! Raw `mio`, zero-runtime RFC 6455 server that plugs into the same
//! thread-per-core philosophy as engine-net and engine-http:
//!
//! - `SO_REUSEPORT` listener — spawn N poll threads on one port, no locks
//! - Handshake reuses the engine-http request parser (one framework, one parser)
//! - Own SHA-1 + base64 (no external crypto deps; handshake-only, not secrets)
//! - Incremental frame decoder: 7/16/64-bit lengths, masking, fragmentation,
//!   ping/pong auto-answer, close handshake
//! - `poll_events(timeout, &mut buf)` appends [`WsEvent`]s to a caller-owned
//!   buffer (zero per-poll allocation); `send_text`/`send_binary`/
//!   `broadcast`/`close` write API with per-connection backpressure limits

pub mod base64;
pub mod frame;
pub mod handshake;
pub mod server;
pub mod sha1;

pub use frame::{Frame, FrameDecoder, opcode};
pub use handshake::{HandshakeReader, accept_key};
pub use server::{WsConfig, WsEvent, WsServer};
