use ahash::AHashMap;
use engine_config::RoomVariantDef;
use std::borrow::Cow;
use std::time::Duration;

use bytes::Bytes;
use mio::net::TcpListener;
use mio::{Poll, Token};

use super::client::WsClient;
use super::error::MatchMakeError;
use crate::transport::TransportEvent;

pub(super) type MatchmakeHook =
    Box<dyn Fn(&str, &str, &str) -> Result<String, MatchMakeError> + Send>;

/// Everything the transport remembers about one connected client, kept in a
/// single map entry instead of four parallel maps keyed by the same session
/// string. One lookup and one key allocation per client instead of four — at
/// 10k clients that is 30k fewer entries and 30k fewer `Arc<str>` clones.
pub(super) struct ClientMeta {
    /// Tokio/mio registration token for the socket.
    pub(super) token: Token,
    /// Room the client joined; `None` until the upgrade carries a room name.
    pub(super) room: Option<std::sync::Arc<str>>,
    /// Raw `joinOrCreate` options blob, kept verbatim for the host.
    pub(super) options: Option<std::sync::Arc<str>>,
    /// Seat token minted by matchmake for this session.
    pub(super) seat: Option<String>,
}

impl Default for ClientMeta {
    fn default() -> Self {
        // Token 0 is never a live registration: real tokens start at 1 via
        // `next_token`. A default meta is always overwritten before use.
        Self::new(Token(0))
    }
}

impl ClientMeta {
    pub(super) fn new(token: Token) -> Self {
        Self {
            token,
            room: None,
            options: None,
            seat: None,
        }
    }
}

pub type HttpHook = Box<dyn Fn(&str) -> Option<(Cow<'static, str>, Bytes)> + Send>;

pub(super) struct ServerCtx<'a> {
    pub(super) matchmake_hook: Option<&'a MatchmakeHook>,
    pub(super) http_hook: Option<&'a HttpHook>,
    pub(super) retained: &'a AHashMap<String, String>,
    /// One entry per session carries the seat token, room and options, so
    /// the upgrade path touches a single map instead of four.
    pub(super) clients_meta: &'a mut AHashMap<std::sync::Arc<str>, ClientMeta>,

    pub(super) game_port: u16,
    pub(super) room_variants: &'a std::collections::BTreeMap<String, RoomVariantDef>,

    pub(super) udp_port: u16,

    pub(super) schema_descriptor: &'a [u8],
    pub(super) allowed_origins: &'a [String],
}

pub(super) struct Pass<'a> {
    pub(super) poll: &'a mut Poll,
    pub(super) listeners: &'a mut Vec<TcpListener>,
    pub(super) clients: &'a mut AHashMap<Token, WsClient>,
    pub(super) clients_meta: &'a mut AHashMap<std::sync::Arc<str>, ClientMeta>,
    pub(super) retained: &'a AHashMap<String, String>,
    pub(super) matchmake_hook: &'a mut Option<MatchmakeHook>,
    pub(super) http_hook: &'a mut Option<HttpHook>,
    pub(super) next_token: &'a mut usize,
    pub(super) disconnected: &'a mut Vec<Token>,
    pub(super) buf: &'a mut Vec<TransportEvent>,
    pub(super) game_port: u16,
    pub(super) room_variants: &'a std::collections::BTreeMap<String, RoomVariantDef>,
    pub(super) udp_port: u16,
    pub(super) schema_descriptor: &'a [u8],
    pub(super) allowed_origins: &'a [String],
    pub(super) simulated_latency: Duration,
}
