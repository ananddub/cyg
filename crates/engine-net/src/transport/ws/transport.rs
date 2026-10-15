//! Transport lifecycle: the `mio` poll, the listening sockets it owns and the
//! maps that name every live session.

use std::borrow::Cow;
use std::collections::HashMap;
use std::io;
use std::net::SocketAddr;

use bytes::{Bytes, BytesMut};
use mio::net::TcpListener;
use mio::{Events, Interest, Poll, Token};
use socket2::{Domain, Protocol, Socket, Type};

use super::client::WsClient;
use super::error::MatchMakeError;
use super::heartbeat::HeartbeatConfig;
use super::hooks::{HttpHook, MatchmakeHook};
use crate::transport::TransportEvent;

pub struct WsTransport {
    pub(super) poll: Poll,
    pub(super) events: Events,
    pub(super) listeners: Vec<TcpListener>,
    pub(super) clients: HashMap<Token, WsClient>,
    pub(super) client_tokens: HashMap<String, Token>,
    /// session id -> room id (from the upgrade URL path).
    pub(super) client_rooms: HashMap<String, String>,
    /// Abnormally-disconnected sessions kept alive for reconnection:
    /// reconnectionToken ("roomId:sessionId") -> session id.
    pub(super) retained: HashMap<String, String>,
    /// session id -> reconnectionToken advertised in its matchmake seat, so
    /// the JOIN_ROOM handshake echoes the same token the SDK will reconnect with.
    pub(super) seat_tokens: HashMap<String, String>,
    /// Events queued outside poll_events (e.g. `disconnect()`), drained first.
    pub(super) pending_events: Vec<TransportEvent>,
    /// Cross-poll event buffer; handed to `poll_events` via `mem::take` so
    /// helper calls can still take `&mut self`.
    pub(super) ev: Vec<TransportEvent>,
    /// Dead tokens collected during a poll; persists across polls. The id is
    /// taken from the removed client, so no id clones happen here.
    pub(super) disconnected: Vec<Token>,
    /// Heartbeat kill list; persists across polls.
    pub(super) hb_kills: Vec<Token>,
    /// Broadcast encode scratch; reuses its allocation.
    pub(super) sb: BytesMut,
    pub(super) heartbeat: HeartbeatConfig,
    pub(super) next_token: usize,
    /// Actual kernel-assigned bound address (resolves port 0).
    pub(super) bound_addr: SocketAddr,
    pub bound_addrs: Vec<SocketAddr>,
    pub(super) matchmake_hook: Option<MatchmakeHook>,
    pub(super) http_hook: Option<HttpHook>,
}

impl WsTransport {
    pub fn bound_addrs(&self) -> &[SocketAddr] {
        &self.bound_addrs
    }

    pub fn new(bind_addr: SocketAddr) -> io::Result<Self> {
        Self::new_multi(&[bind_addr])
    }

    pub fn new_multi(bind_addrs: &[SocketAddr]) -> io::Result<Self> {
        let poll = Poll::new()?;
        let events = Events::with_capacity(1024);
        let mut listeners = Vec::new();
        let mut resolved_addrs = Vec::new();

        for (idx, &addr) in bind_addrs.iter().enumerate() {
            let socket = Socket::new(Domain::IPV4, Type::STREAM, Some(Protocol::TCP))?;
            socket.set_reuse_address(true)?;
            #[cfg(all(unix, not(target_os = "solaris"), not(target_os = "illumos")))]
            socket.set_reuse_port(true)?;
            socket.set_nonblocking(true)?;
            socket.set_tcp_nodelay(true)?;

            let sock_addr = addr.into();
            socket.bind(&sock_addr)?;
            socket.listen(1024)?;

            let std_listener: std::net::TcpListener = socket.into();
            let mut listener = TcpListener::from_std(std_listener);
            poll.registry()
                .register(&mut listener, Token(idx), Interest::READABLE)?;

            resolved_addrs.push(listener.local_addr()?);
            listeners.push(listener);
        }

        let bound_addr = resolved_addrs
            .first()
            .copied()
            .unwrap_or_else(|| "0.0.0.0:0".parse().unwrap());
        let next_token = listeners.len() + 10;

        Ok(Self {
            poll,
            events,
            listeners,
            clients: HashMap::new(),
            client_tokens: HashMap::new(),
            client_rooms: HashMap::new(),
            retained: HashMap::new(),
            seat_tokens: HashMap::new(),
            pending_events: Vec::new(),
            ev: Vec::new(),
            disconnected: Vec::new(),
            hb_kills: Vec::new(),
            sb: BytesMut::with_capacity(1024),
            heartbeat: HeartbeatConfig::default(),
            next_token,
            bound_addr,
            bound_addrs: resolved_addrs,
            matchmake_hook: None,
            http_hook: None,
        })
    }

    /// Override the unresponsive-client heartbeat (see [`HeartbeatConfig`]).
    pub fn set_heartbeat(&mut self, config: HeartbeatConfig) {
        self.heartbeat = config;
    }

    /// Register static HTTP request handler: (path) -> Option<(content_type, body)>
    pub fn on_http(
        &mut self,
        f: impl Fn(&str) -> Option<(Cow<'static, str>, Bytes)> + Send + 'static,
    ) {
        self.http_hook = Some(Box::new(f));
    }

    /// Register the game's matchmake handler: receives (method, room, body)
    /// and returns the seat-reservation JSON, or a typed rejection that the
    /// SDK surfaces as a `MatchMakeError`.
    pub fn on_matchmake(
        &mut self,
        f: impl Fn(&str, &str, &str) -> Result<String, MatchMakeError> + Send + 'static,
    ) {
        self.matchmake_hook = Some(Box::new(f));
    }

    /// Which room a connected session belongs to.
    pub fn room_of(&self, client_id: &str) -> Option<&str> {
        self.client_rooms.get(client_id).map(String::as_str)
    }

    /// Keep a disconnected session's reconnection token valid so a
    /// `matchmake/reconnect` can re-issue its seat reservation.
    pub fn retain_session(&mut self, room_id: &str, session_id: &str) {
        self.retained.insert(
            format!("{}:{}", room_id, session_id),
            session_id.to_string(),
        );
    }
}
