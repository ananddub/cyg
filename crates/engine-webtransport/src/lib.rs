//! engine-webtransport — WebTransport-over-HTTP/3 with zero async runtime.
//!
//! Built on raw `mio` (one `Poll`, one UDP socket, thread-per-socket) and
//! `quiche` for QUIC. Unlike the `wtransport` crate there is no tokio
//! anywhere in the stack; the HTTP/3 layer is intentionally minimal:
//!
//! * Control stream advertises SETTINGS with `ENABLE_CONNECT_PROTOCOL=1`,
//!   `ENABLE_WEBTRANSPORT=1`, `H3_DATAGRAM=1` and QPACK dynamic table
//!   capacity 0, which makes the stateless QPACK codec (`qpack` crate) a
//!   fully spec-compliant header serializer.
//! * Sessions are created by extended CONNECT `:protocol: webtransport`;
//!   the CONNECT request stream doubles as the capsule channel
//!   (CLOSE_WEBTRANSPORT_SESSION 0x2843d064).
//! * WT streams carry the draft stream prefixes: bidi `0x41`, client uni
//!   `0x51`, server uni `0x54`, each followed by the session id varint.
//! * WT datagrams use the length-prefixed record framing:
//!   `varint record-len | varint session-id | payload`.
//!
//! Header blocks are never hand-assembled: use [`WtRequest`] /
//! [`WtResponse`] builders (`to_fields()` / `from_fields()`).
//!
//! ```no_run
//! use std::time::Duration;
//! use engine_webtransport::{WebTransportServer, WtConfig, WtEvent};
//!
//! let mut srv = WebTransportServer::new(WtConfig::default()).unwrap();
//! // Caller-owned event buffer: reused across polls, no per-tick allocation.
//! let mut events: Vec<WtEvent> = Vec::new();
//! loop {
//!     srv.poll_events(Duration::from_millis(100), &mut events).unwrap();
//!     for ev in events.drain(..) {
//!         match ev {
//!             WtEvent::SessionOpened { session, .. } => srv.accept_session(session).unwrap(),
//!             WtEvent::StreamData { session, stream, data, fin } => {
//!                 srv.stream_send(session, stream, &data, false).unwrap();
//!                 if fin { srv.stream_close(session, stream).unwrap(); }
//!             }
//!             WtEvent::Datagram { session, data } => { srv.send_datagram(session, &data).unwrap(); }
//!             _ => {}
//!         }
//!     }
//! }
//! ```

pub mod h3;
mod headers;
mod server;
pub mod varint;

/// QPACK is re-exported so integrators (and tests) can encode/decode header
/// blocks with the exact same stateless codec the server uses.
pub use ::qpack;

pub use h3::{PeerSettings, capsule_type, frame_type, setting, stream_type};
pub use headers::{HeaderError, WtRequest, WtResponse};
pub use server::{StreamDir, WebTransportServer, WtConfig, WtEvent};
pub use varint::MAX_VALUE as VARINT_MAX_VALUE;
