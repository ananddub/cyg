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

use std::borrow::Cow;
use std::collections::HashMap;

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
    pub(super) retained: &'a HashMap<String, String>,
    pub(super) seat_tokens: &'a mut HashMap<String, String>,
    pub(super) client_rooms: &'a mut HashMap<String, String>,
}

/// Everything one `poll_events` pass mutates: field borrows of the transport
/// plus the buffers adopted for the duration of the pass.
pub(super) struct Pass<'a> {
    pub(super) poll: &'a mut Poll,
    pub(super) listeners: &'a mut Vec<TcpListener>,
    pub(super) clients: &'a mut HashMap<Token, WsClient>,
    pub(super) client_tokens: &'a mut HashMap<String, Token>,
    pub(super) client_rooms: &'a mut HashMap<String, String>,
    pub(super) retained: &'a HashMap<String, String>,
    pub(super) seat_tokens: &'a mut HashMap<String, String>,
    pub(super) matchmake_hook: &'a mut Option<MatchmakeHook>,
    pub(super) http_hook: &'a mut Option<HttpHook>,
    pub(super) next_token: &'a mut usize,
    pub(super) disconnected: &'a mut Vec<Token>,
    pub(super) buf: &'a mut Vec<TransportEvent>,
}
