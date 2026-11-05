
use ahash::{AHashMap, AHashSet};
use std::io;
use std::net::SocketAddr;
use std::time::{Duration, Instant};

use bytes::{Bytes, BytesMut};
use mio::net::TcpListener;
use mio::{Events, Interest, Poll, Token};
use socket2::{Domain, Protocol, Socket, Type};

use super::client::WsClient;
use super::heartbeat::HeartbeatConfig;
use super::hooks::{HttpHook, MatchmakeHook};
use crate::transport::TransportEvent;

pub struct WsTransport {
    pub(super) poll: Poll,
    pub(super) events: Events,
    pub(super) listeners: Vec<TcpListener>,
    pub(super) clients: AHashMap<Token, WsClient>,
    pub(super) client_tokens: AHashMap<String, Token>,

    pub(super) client_rooms: AHashMap<String, String>,

    pub(super) client_options: AHashMap<String, String>,

    pub(super) consented: AHashSet<String>,

    pub(super) retained: AHashMap<String, String>,

    pub(super) seat_tokens: AHashMap<String, String>,

    pub(super) pending_events: Vec<TransportEvent>,

    pub(super) ev: Vec<TransportEvent>,

    pub(super) disconnected: Vec<Token>,

    pub(super) hb_kills: Vec<Token>,

    pub(super) sb: BytesMut,
    pub(super) heartbeat: HeartbeatConfig,
    pub(super) next_token: usize,

    pub(super) flush_dirty: bool,

    pub(super) dropped_frames: u64,
    pub(super) last_drop_log: Instant,

    pub(super) bound_addr: SocketAddr,
    pub bound_addrs: Vec<SocketAddr>,

    pub(super) advertised_udp_port: u16,
    pub(super) matchmake_hook: Option<MatchmakeHook>,
    /// Named room variants ([rooms.x] in config) — matchmaking on this lane
    /// splits joins into variant room keys, mirroring the control plane.
    pub(super) room_variants: std::collections::BTreeMap<String, engine_config::RoomVariantDef>,
    pub(super) http_hook: Option<HttpHook>,

    pub(super) allowed_origins: Vec<String>,

    pub(super) schema_descriptor: Bytes,
    pub(super) simulated_latency: Duration,
}

impl WsTransport {
    pub fn bound_addrs(&self) -> &[SocketAddr] {
        &self.bound_addrs
    }

    pub fn set_simulated_latency(&mut self, lat: Duration) {
        self.simulated_latency = lat;
    }

    pub(super) fn flush_delayed_pings(&mut self, now: Instant) {
        for client in self.clients.values_mut() {
            if !client.delayed_pings.is_empty() {
                let mut i = 0;
                while i < client.delayed_pings.len() {
                    if client.delayed_pings[i].0 <= now {
                        let (_, payload) = client.delayed_pings.swap_remove(i);
                        super::rfc6455::queue_message(client, super::protocol::PROTO_PING, &payload);
                        self.flush_dirty = true;
                    } else {
                        i += 1;
                    }
                }
            }
        }
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
            clients: AHashMap::new(),
            client_tokens: AHashMap::new(),
            client_rooms: AHashMap::new(),
            client_options: AHashMap::new(),
            consented: AHashSet::new(),
            retained: AHashMap::new(),
            seat_tokens: AHashMap::new(),
            pending_events: Vec::new(),
            ev: Vec::new(),
            disconnected: Vec::new(),
            hb_kills: Vec::new(),
            sb: BytesMut::with_capacity(1024),
            heartbeat: HeartbeatConfig::default(),
            next_token,
            flush_dirty: false,
            dropped_frames: 0,
            last_drop_log: Instant::now(),
            bound_addr,
            bound_addrs: resolved_addrs,
            advertised_udp_port: 7778,
            matchmake_hook: None,
            room_variants: Default::default(),
            http_hook: None,
            allowed_origins: Vec::new(),
            schema_descriptor: Bytes::new(),
            simulated_latency: Duration::ZERO,
        })
    }
}
