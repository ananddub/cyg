
use engine_config::RoomVariantDef;
use ahash::AHashMap;
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

pub type HttpHook = Box<dyn Fn(&str) -> Option<(Cow<'static, str>, Bytes)> + Send>;

pub(super) struct ServerCtx<'a> {
    pub(super) matchmake_hook: Option<&'a MatchmakeHook>,
    pub(super) http_hook: Option<&'a HttpHook>,
    pub(super) retained: &'a AHashMap<String, String>,
    pub(super) seat_tokens: &'a mut AHashMap<String, String>,
    pub(super) client_rooms: &'a mut AHashMap<String, String>,

    pub(super) client_options: &'a mut AHashMap<String, String>,

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
    pub(super) room_variants: &'a std::collections::BTreeMap<String, RoomVariantDef>,
    pub(super) udp_port: u16,
    pub(super) schema_descriptor: &'a [u8],
    pub(super) allowed_origins: &'a [String],
    pub(super) simulated_latency: Duration,
}
