//! Game-supplied hooks and the borrow bundles that hand transport state to
//! helpers: [`ServerCtx`] for one greeting, [`Pass`] for one `poll_events`
//! iteration.
//!
//! Both exist because `mio::Events::iter` holds an immutable borrow of the event
//! set for the whole pass, so a helper cannot take `&mut WsTransport` alongside
//! it. They take a struct of field-level borrows instead — and a `ServerCtx` is
//! built by naming those fields directly, never by a method on `Pass`, because a
//! method call reborrows the whole struct and would clash with the client borrow
//! the caller is already holding.

use ahash::AHashMap;
use std::borrow::Cow;

use bytes::Bytes;
use mio::net::TcpListener;
use mio::{Poll, Token};

use super::client::WsClient;
use super::error::MatchMakeError;
use crate::transport::TransportEvent;

/// Game hook for matchmake requests: (method, room, body) -> seat-reservation
/// JSON, or a typed rejection (sent as a `MatchMakeError` to the SDK).
pub(super) type MatchmakeHook =
    Box<dyn Fn(&str, &str, &str) -> Result<String, MatchMakeError> + Send>;

/// Static HTTP hook: (path) -> Option<(content_type, body)>
///
/// Both halves are cheap-to-hand-off values: a borrowed content type and a
/// refcounted body, so serving the same asset to 500 clients copies nothing.
pub type HttpHook = Box<dyn Fn(&str) -> Option<(Cow<'static, str>, Bytes)> + Send>;

/// Borrow bundle handed to `handle_client_greeting` so the per-event client
/// borrow never overlaps the shared transport state it needs.
pub(super) struct ServerCtx<'a> {
    pub(super) matchmake_hook: Option<&'a MatchmakeHook>,
    pub(super) http_hook: Option<&'a HttpHook>,
    pub(super) retained: &'a AHashMap<String, String>,
    pub(super) seat_tokens: &'a mut AHashMap<String, String>,
    pub(super) client_rooms: &'a mut AHashMap<String, String>,
    /// session id -> raw join options JSON from the upgrade URL.
    pub(super) client_options: &'a mut AHashMap<String, String>,
    /// Port the SDK must dial for the game WebSocket.
    pub(super) game_port: u16,
    /// UDP port advertised to native clients.
    pub(super) udp_port: u16,
    /// Compact schema descriptor pushed right after the JOIN_ROOM handshake.
    /// Empty when the project has no generated schema.
    pub(super) schema_descriptor: &'a [u8],
    /// Browser origins allowed on matchmake; see `WsTransport::set_allowed_origins`.
    pub(super) allowed_origins: &'a [String],
}

/// Everything one `poll_events` pass mutates: field borrows of the transport
/// plus the buffers adopted for the duration of the pass.
pub(super) struct Pass<'a> {
    pub(super) poll: &'a mut Poll,
    pub(super) listeners: &'a mut Vec<TcpListener>,
    pub(super) clients: &'a mut AHashMap<Token, WsClient>,
    pub(super) client_tokens: &'a mut AHashMap<String, Token>,
    pub(super) client_rooms: &'a mut AHashMap<String, String>,
    pub(super) retained: &'a AHashMap<String, String>,
    pub(super) seat_tokens: &'a mut AHashMap<String, String>,
    pub(super) client_options: &'a mut AHashMap<String, String>,
    pub(super) matchmake_hook: &'a mut Option<MatchmakeHook>,
    pub(super) http_hook: &'a mut Option<HttpHook>,
    pub(super) next_token: &'a mut usize,
    pub(super) disconnected: &'a mut Vec<Token>,
    pub(super) buf: &'a mut Vec<TransportEvent>,
    pub(super) game_port: u16,
    pub(super) udp_port: u16,
    pub(super) schema_descriptor: &'a [u8],
    pub(super) allowed_origins: &'a [String],
}
