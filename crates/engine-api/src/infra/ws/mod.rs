//! The realtime wire: `/ws`.
//!
//! BaaS parity: `server/session_ws.go` plus the `pipeline_*` handlers. One JSON
//! [`envelope::Envelope`] per text frame; the connection authenticates at upgrade time
//! with `?token=<jwt>` (a `Bearer` header is also accepted), replies carry the request's
//! `cid`, and failures arrive as a `type:"error"` frame carrying the message.
//!
//! [`envelope`] is the frame shape, [`protocol`] the codec and upgrade authentication,
//! [`session`] the registry of connected sockets. The event table is the one
//! `#[auto_ws("/ws")]` impl in [`crate::controllers::realtime`]; plane logic is in
//! `crate::services`. The framework's writer task owns the socket sink, so a slow client
//! never blocks the read loop. On teardown a session leaves its matches, channels and
//! party, drops its matchmaker tickets, and tells status followers it went offline.

pub mod envelope;
pub mod protocol;
pub mod session;

pub use protocol::WsProtocol;
