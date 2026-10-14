//! # Colyseus WebSocket Wire-Protocol Transport
//!
//! Drop-in transport supporting `@colyseus/sdk` clients without client-side migration:
//! - Standard RFC 6455 WebSocket Upgrade handshake (`101 Switching Protocols`)
//! - RFC 6455 binary frame decoding with client unmasking, fragmentation,
//!   control frames (`ping`/`pong`/`close`) and server heartbeat
//! - Matchmake seat-reservation endpoint (`POST /matchmake/<method>/<room>`)
//!   with the full Colyseus method whitelist (`joinOrCreate`, `create`, `join`,
//!   `joinById`, `reconnect`) and typed `MatchMakeError` rejections
//! - Colyseus protocol opcodes (packages/shared-types/src/Protocol.ts), with
//!   modifier bits (`PROTOCOL_CODE_MASK`) stripped before dispatch:
//!   - `10` -> `JOIN_ROOM` handshake (server-initiated right after upgrade)
//!   - `11` -> `ERROR` (server-initiated, surfaces to `room.onError`)
//!   - `12` -> `LEAVE_ROOM` (client leave; server answers close `4000 CONSENTED`)
//!   - `13` -> `ROOM_DATA` (client messages -> game layer)
//!   - `14` -> `ROOM_STATE` (full snapshots via `OpCode::StateSnapshot`)
//!   - `15` -> `ROOM_STATE_PATCH` (server state deltas via `OpCode::StateDelta`)
//!   - `17` -> `ROOM_DATA_BYTES` (binary messages via `OpCode::RoomDataBytes`)
//!   - `18` -> `PING` (echoed back — powers `room.ping()` and `client.getLatency()`)
//!   - `21` -> `ROOM_REQUEST` (client request/response RPC via `OpCode::RoomRequest`)
//!   - `22` -> `ROOM_RESPONSE` (server reply via `OpCode::RoomResponse`)
//! - Reconnection: disconnecting sessions keep their reconnection token valid
//!   (`retain_session`) so `matchmake/reconnect` can re-issue the seat

use std::borrow::Cow;
use std::collections::HashMap;
use std::io::{self, Write};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use bytes::{Buf, BufMut, Bytes, BytesMut};
use mio::net::TcpListener;
use mio::{Events, Interest, Poll, Token};
use socket2::{Domain, Protocol, Socket, Type};

use super::{GameTransport, TransportEvent};
use crate::colyseus_state::{mp_number, mp_str};
use crate::framing::OpCode;

#[allow(dead_code)]
const LISTENER_TOKEN: Token = Token(0);
#[allow(dead_code)]
const CLIENT_TOKEN_START: usize = 1;

// Colyseus Protocol Wire Constants (packages/shared-types/src/Protocol.ts)
// NOTE: this protocol version has NO separate JOIN_REQUEST — the client sends
// JOIN_ROOM (10) directly. 11 is ERROR, not JOIN_ROOM.
const COLYSEUS_JOIN_ROOM: u8 = 10;
const COLYSEUS_ERROR: u8 = 11;
const COLYSEUS_LEAVE_ROOM: u8 = 12;
const COLYSEUS_ROOM_DATA: u8 = 13;
const COLYSEUS_ROOM_STATE: u8 = 14;
const COLYSEUS_ROOM_STATE_PATCH: u8 = 15;
const COLYSEUS_ROOM_DATA_BYTES: u8 = 17;
const COLYSEUS_PING: u8 = 18;
const COLYSEUS_ROOM_REQUEST: u8 = 21;
const COLYSEUS_ROOM_RESPONSE: u8 = 22;

/// Modifier bits share the leading protocol byte (bits 5..7). Decoders strip
/// them before dispatching: `code = buffer[0] & PROTOCOL_CODE_MASK`.
const PROTOCOL_CODE_MASK: u8 = 0x1F;

/// Colyseus WebSocket close codes (shared-types CloseCode).
pub mod close_code {
    pub const CONSENTED: u16 = 4000;
    pub const SERVER_SHUTDOWN: u16 = 4001;
    pub const WITH_ERROR: u16 = 4002;
    pub const FAILED_TO_RECONNECT: u16 = 4003;
    pub const MAY_TRY_RECONNECT: u16 = 4010;
}

/// Status byte of a `ROOM_RESPONSE` (shared-types ResponseStatus).
pub mod response_status {
    pub const OK: u8 = 0;
    pub const REJECTED: u8 = 1;
    pub const ERROR: u8 = 2;
}

/// The matchmake methods real Colyseus serves over HTTP.
pub const MATCHMAKE_METHODS: [&str; 5] =
    ["joinOrCreate", "create", "join", "joinById", "reconnect"];

/// Serializer advertised to SDK clients. `"schema"` makes the client boot its
/// @colyseus/schema decoder; with an empty state reflection (len 0) the client
/// accepts `any`-typed rooms without needing a matching schema class.
const SERIALIZER_ID: &[u8] = b"schema";

/// A typed matchmake rejection, mapped onto the Colyseus HTTP error codes
/// (shared-types ErrorCode) and surfaced to the SDK as a `MatchMakeError`.
#[derive(Debug, Clone)]
pub struct MatchMakeError {
    pub code: u16,
    pub message: String,
}

impl MatchMakeError {
    pub fn new(code: u16, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
    pub fn no_handler(message: impl Into<String>) -> Self {
        Self::new(520, message)
    }
    pub fn invalid_criteria(message: impl Into<String>) -> Self {
        Self::new(521, message)
    }
    pub fn invalid_room_id(message: impl Into<String>) -> Self {
        Self::new(522, message)
    }
    pub fn unhandled(message: impl Into<String>) -> Self {
        Self::new(523, message)
    }
    pub fn expired(message: impl Into<String>) -> Self {
        Self::new(524, message)
    }
    pub fn auth_failed(message: impl Into<String>) -> Self {
        Self::new(525, message)
    }
    pub fn application_error(message: impl Into<String>) -> Self {
        Self::new(526, message)
    }
}

impl From<&str> for MatchMakeError {
    fn from(message: &str) -> Self {
        Self::unhandled(message)
    }
}

impl From<String> for MatchMakeError {
    fn from(message: String) -> Self {
        Self::unhandled(message)
    }
}

/// Server-side heartbeat (mirrors Colyseus `autoTerminateUnresponsiveClients`):
/// WS ping frames are sent to idle clients; a client that misses more than
/// `max_retries` pongs is terminated. Browsers answer WS pings in the network
/// stack, so real SDK clients are safe even in throttled background tabs.
#[derive(Debug, Clone, Copy)]
pub struct HeartbeatConfig {
    pub enabled: bool,
    pub ping_interval: Duration,
    pub max_retries: u32,
}

impl Default for HeartbeatConfig {
    fn default() -> Self {
        // Colyseus ws-transport defaults: pingInterval 3000ms, pingMaxRetries 2.
        Self {
            enabled: true,
            ping_interval: Duration::from_millis(3000),
            max_retries: 2,
        }
    }
}

/// Game hook for matchmake requests: (method, room, body) -> seat-reservation
/// JSON, or a typed rejection (sent as a `MatchMakeError` to the SDK).
type MatchmakeHook = Box<dyn Fn(&str, &str, &str) -> Result<String, MatchMakeError> + Send>;

/// Static HTTP hook: (path) -> Option<(content_type, body)>
///
/// Both halves are cheap-to-hand-off values: a borrowed content type and a
/// refcounted body, so serving the same asset to 500 clients copies nothing.
pub type HttpHook = Box<dyn Fn(&str) -> Option<(Cow<'static, str>, Bytes)> + Send>;

/// State of a connected Colyseus SDK client.
#[derive(PartialEq)]
enum WsState {
    Handshaking,
    Connected,
}

struct ColyseusClient {
    stream: mio::net::TcpStream,
    state: WsState,
    session_id: Arc<str>,
    /// Room id from the upgrade URL path (/processId/roomId?...).
    room_id: String,
    recv_buf: BytesMut,
    send_queue: BytesMut,
    /// Inbound fragmented message accumulation (RFC 6455 continuation frames).
    frag_opcode: Option<u8>,
    frag_buf: BytesMut,
    // --- heartbeat bookkeeping ---
    last_activity: Instant,
    ping_sent_at: Option<Instant>,
    missed_pings: u32,
}

/// Borrow bundle handed to `handle_client_greeting` so the per-event client
/// borrow never overlaps the shared transport state it needs.
struct ServerCtx<'a> {
    matchmake_hook: Option<&'a MatchmakeHook>,
    http_hook: Option<&'a HttpHook>,
    retained: &'a HashMap<String, String>,
    seat_tokens: &'a mut HashMap<String, String>,
    client_rooms: &'a mut HashMap<String, String>,
}

pub struct ColyseusWsTransport {
    poll: Poll,
    events: Events,
    listeners: Vec<TcpListener>,
    clients: HashMap<Token, ColyseusClient>,
    client_tokens: HashMap<String, Token>,
    /// session id -> room id (from the upgrade URL path).
    client_rooms: HashMap<String, String>,
    /// Abnormally-disconnected sessions kept alive for reconnection:
    /// reconnectionToken ("roomId:sessionId") -> session id.
    retained: HashMap<String, String>,
    /// session id -> reconnectionToken advertised in its matchmake seat, so
    /// the JOIN_ROOM handshake echoes the same token the SDK will reconnect with.
    seat_tokens: HashMap<String, String>,
    /// Events queued outside poll_events (e.g. `disconnect()`), drained first.
    pending_events: Vec<TransportEvent>,
    /// Cross-poll event buffer; handed to `poll_events` via `mem::take` so
    /// helper calls can still take `&mut self`.
    ev: Vec<TransportEvent>,
    /// Dead tokens collected during a poll; persists across polls. The id is
    /// taken from the removed client, so no id clones happen here.
    disconnected: Vec<Token>,
    /// Heartbeat kill list; persists across polls.
    hb_kills: Vec<Token>,
    /// Broadcast encode scratch; reuses its allocation.
    sb: bytes::BytesMut,
    heartbeat: HeartbeatConfig,
    next_token: usize,
    /// Actual kernel-assigned bound address (resolves port 0).
    bound_addr: SocketAddr,
    pub bound_addrs: Vec<SocketAddr>,
    matchmake_hook: Option<MatchmakeHook>,
    http_hook: Option<HttpHook>,
}

impl ColyseusWsTransport {
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
            sb: bytes::BytesMut::with_capacity(1024),
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

    /// Send a Colyseus `ERROR` (11) frame: surfaces to the SDK's
    /// `room.onError` handler. Wire: `[11][msgpack code][msgpack string]`.
    pub fn send_error(&mut self, client_id: &str, code: u16, message: &str) -> io::Result<()> {
        let mut payload = std::mem::take(&mut self.sb);
        payload.reserve(8 + message.len());
        mp_number(&mut payload, code as f64);
        mp_str(&mut payload, message);
        self.queue_to(client_id, COLYSEUS_ERROR, &payload);
        payload.clear();
        self.sb = payload;
        Ok(())
    }

    /// Reply to a `ROOM_REQUEST` (21). Wire:
    /// `[22][msgpack requestId][status byte][msgpack payload]`.
    pub fn send_room_response(
        &mut self,
        client_id: &str,
        request_id: u32,
        status: u8,
        payload_msgpack: &[u8],
    ) -> io::Result<()> {
        let mut payload = std::mem::take(&mut self.sb);
        payload.reserve(8 + payload_msgpack.len());
        mp_number(&mut payload, request_id as f64);
        payload.put_u8(status);
        payload.extend_from_slice(payload_msgpack);
        self.queue_to(client_id, COLYSEUS_ROOM_RESPONSE, &payload);
        payload.clear();
        self.sb = payload;
        Ok(())
    }

    /// Queue one Colyseus message (opcode byte + payload) as a WS frame for
    /// the given session.
    fn queue_to(&mut self, client_id: &str, colyseus_opcode: u8, payload: &[u8]) {
        if let Some(&token) = self.client_tokens.get(client_id) {
            if let Some(client) = self.clients.get_mut(&token) {
                encode_colyseus_frame(colyseus_opcode, payload, &mut client.send_queue);
            }
        }
    }

    /// Drop a client immediately: sends the WS close frame (`4000 CONSENTED`)
    /// and surfaces a `Disconnected` event on the next `poll_events`.
    pub fn disconnect(&mut self, client_id: &str) -> io::Result<()> {
        if let Some(&token) = self.client_tokens.get(client_id) {
            if let Some(mut client) = self.clients.remove(&token) {
                queue_close_frame(&mut client, close_code::CONSENTED, "server disconnect");
                let _ = self.poll.registry().deregister(&mut client.stream);
                finish_close(client);
            }
            self.client_tokens.remove(client_id);
            self.client_rooms.remove(client_id);
            self.seat_tokens.remove(client_id);
            self.pending_events.push(TransportEvent::Disconnected {
                client_id: Arc::from(client_id),
            });
        }
        Ok(())
    }

    /// Send WS ping frames to idle clients and terminate the unresponsive
    /// ones. Killed tokens are collected in `self.hb_kills` (reused across
    /// polls, no allocation).
    fn run_heartbeat(&mut self, now: Instant) {
        let (hb, clients) = (&mut self.hb_kills, &mut self.clients);
        hb.clear();
        if !self.heartbeat.enabled {
            return;
        }
        for (token, client) in clients.iter_mut() {
            if client.state != WsState::Connected {
                continue;
            }
            match client.ping_sent_at {
                Some(sent_at) => {
                    if now.duration_since(sent_at) >= self.heartbeat.ping_interval {
                        client.missed_pings += 1;
                        if client.missed_pings > self.heartbeat.max_retries {
                            hb.push(*token);
                            continue;
                        }
                        send_ws_ping(client);
                        client.ping_sent_at = Some(now);
                    }
                }
                None => {
                    if now.duration_since(client.last_activity) >= self.heartbeat.ping_interval {
                        send_ws_ping(client);
                        client.ping_sent_at = Some(now);
                    }
                }
            }
        }
    }
}

impl GameTransport for ColyseusWsTransport {
    fn poll_events(&mut self, timeout: Duration, ev: &mut Vec<TransportEvent>) -> io::Result<()> {
        self.poll.poll(&mut self.events, Some(timeout))?;

        let now = Instant::now();

        // Adopt the persistent cross-poll buffer, seeded with events queued
        // outside poll_events (e.g. `disconnect()`).
        let mut buf = std::mem::take(&mut self.ev);
        std::mem::swap(&mut buf, &mut self.pending_events);
        self.pending_events.clear();

        // Heartbeat: terminate clients that stopped answering pings.
        self.run_heartbeat(now);
        let mut hb_kills = std::mem::take(&mut self.hb_kills);
        for token in hb_kills.iter() {
            if let Some(mut client) = self.clients.remove(token) {
                let cid = client.session_id.clone();
                let _ = self.poll.registry().deregister(&mut client.stream);
                self.client_tokens.remove(&*cid);
                self.client_rooms.remove(&*cid);
                self.seat_tokens.remove(&*cid);
                buf.push(TransportEvent::Disconnected { client_id: cid });
            }
        }
        hb_kills.clear();
        self.hb_kills = hb_kills;

        // Helpers below take `&mut self`, so the maps are adopted for the
        // duration of each pass and restored afterwards (no fresh Vecs).
        let mut clients = std::mem::take(&mut self.clients);
        let mut disconnected = std::mem::take(&mut self.disconnected);
        for event in self.events.iter() {
            let tok = event.token();
            if tok.0 < self.listeners.len() {
                let listener_idx = tok.0;
                loop {
                    match self.listeners[listener_idx].accept() {
                        Ok((mut stream, peer)) => {
                            let token = Token(self.next_token);
                            self.next_token += 1;
                            stream.set_nodelay(true)?;

                            self.poll.registry().register(
                                &mut stream,
                                token,
                                Interest::READABLE | Interest::WRITABLE,
                            )?;

                            // Read the client's first bytes (HTTP request)
                            // straight into the BytesMut — no stack copy.
                            let mut first_bytes: BytesMut = BytesMut::new();
                            loop {
                                match crate::bufio::read_tcp(&mut stream, &mut first_bytes, 4096) {
                                    Ok(0) => break,
                                    Ok(_) => continue,
                                    Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => break,
                                    Err(_) => break,
                                }
                            }

                            // Session id is finalized inside the greeting (the
                            // SDK appends its seat-reservation sessionId to the
                            // upgrade URL). Track token -> id for send_to().
                            let has_first_bytes = !first_bytes.is_empty();
                            let mut client = ColyseusClient {
                                stream,
                                state: WsState::Handshaking,
                                session_id: format!("colyseus_{}_{}", token.0, peer.port()).into(),
                                room_id: String::new(), // filled by greeting
                                recv_buf: first_bytes,
                                send_queue: BytesMut::with_capacity(4096),
                                frag_opcode: None,
                                frag_buf: BytesMut::new(),
                                last_activity: now,
                                ping_sent_at: None,
                                missed_pings: 0,
                            };

                            if has_first_bytes {
                                let close = {
                                    let mut ctx = ServerCtx {
                                        matchmake_hook: self.matchmake_hook.as_ref(),
                                        http_hook: self.http_hook.as_ref(),
                                        retained: &self.retained,
                                        seat_tokens: &mut self.seat_tokens,
                                        client_rooms: &mut self.client_rooms,
                                    };
                                    handle_client_greeting(
                                        &mut client,
                                        &peer.to_string(),
                                        &mut buf,
                                        &mut ctx,
                                    )
                                };
                                if close {
                                    // HTTP request served; nothing to track.
                                    continue;
                                }
                                // Frames may have arrived together with the
                                // upgrade request (edge-triggered epoll).
                                if client.state == WsState::Connected && !client.recv_buf.is_empty()
                                {
                                    process_ws_frames(&mut client, &mut buf, now);
                                }
                            }
                            // Real WebSocket client: keep it registered (data
                            // may not have arrived yet — greeting re-runs on
                            // the readable event).
                            self.client_tokens
                                .insert(client.session_id.to_string(), token);
                            clients.insert(token, client);
                        }
                        Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => break,
                        Err(e) => return Err(e),
                    }
                }
            } else {
                let token = tok;
                let mut conn_dead = false;

                if let Some(client) = clients.get_mut(&token) {
                    if event.is_readable() {
                        loop {
                            match crate::bufio::read_tcp(
                                &mut client.stream,
                                &mut client.recv_buf,
                                4096,
                            ) {
                                Ok(0) => {
                                    conn_dead = true;
                                    break;
                                }
                                Ok(_) => {}
                                Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => break,
                                Err(_) => {
                                    conn_dead = true;
                                    break;
                                }
                            }
                        }

                        match client.state {
                            WsState::Handshaking => {
                                let prev_id = client.session_id.clone();
                                let peer = client
                                    .stream
                                    .peer_addr()
                                    .map(|a| a.to_string())
                                    .unwrap_or_default();
                                let closed = {
                                    let mut ctx = ServerCtx {
                                        matchmake_hook: self.matchmake_hook.as_ref(),
                                        http_hook: self.http_hook.as_ref(),
                                        retained: &self.retained,
                                        seat_tokens: &mut self.seat_tokens,
                                        client_rooms: &mut self.client_rooms,
                                    };
                                    handle_client_greeting(client, &peer, &mut buf, &mut ctx)
                                };
                                if closed {
                                    conn_dead = true;
                                } else {
                                    // Greeting may have adopted the SDK's
                                    // seat-reservation sessionId — re-key the
                                    // token map so send_to() finds it.
                                    if client.session_id != prev_id {
                                        self.client_tokens.remove(&*prev_id);
                                        self.client_tokens
                                            .insert(client.session_id.to_string(), token);
                                    }
                                    if client.state == WsState::Connected {
                                        // Frames may have arrived in the same TCP
                                        // segment as the upgrade request; mio is
                                        // edge-triggered, so drain them now.
                                        conn_dead = process_ws_frames(client, &mut buf, now);
                                    }
                                }
                            }
                            WsState::Connected => {
                                conn_dead = process_ws_frames(client, &mut buf, now);
                            }
                        }
                    }

                    if event.is_writable() && !client.send_queue.is_empty() {
                        match client.stream.write(&client.send_queue) {
                            Ok(n) => {
                                client.send_queue.advance(n);
                            }
                            Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {}
                            Err(_) => conn_dead = true,
                        }
                    }
                }

                if conn_dead {
                    disconnected.push(token);
                }
            }
        }

        for token in disconnected.drain(..) {
            if let Some(mut client) = clients.remove(&token) {
                let cid = client.session_id.clone();
                // Deliver any queued close frame (e.g. LEAVE_ROOM's
                // `4000 CONSENTED`) before the socket dies.
                let _ = self.poll.registry().deregister(&mut client.stream);
                finish_close(client);
                self.client_tokens.remove(&*cid);
                self.client_rooms.remove(&*cid);
                self.seat_tokens.remove(&*cid);
                buf.push(TransportEvent::Disconnected { client_id: cid });
            }
        }
        self.clients = clients;
        self.disconnected = disconnected;

        std::mem::swap(ev, &mut buf);
        buf.clear();
        self.ev = buf;
        Ok(())
    }

    fn send_to(&mut self, client_id: &str, opcode: OpCode, payload: &[u8]) -> io::Result<()> {
        self.queue_to(client_id, colyseus_opcode_for(opcode), payload);
        Ok(())
    }

    fn broadcast(&mut self, opcode: OpCode, payload: &[u8]) -> io::Result<()> {
        let colyseus_opcode = colyseus_opcode_for(opcode);
        let (sb, clients) = (&mut self.sb, &mut self.clients);
        sb.reserve(11 + payload.len());
        sb.put_u8(colyseus_opcode);
        sb.put_slice(payload);
        // Freeze first so the encoder can read the message and write the
        // framed bytes into the same scratch in two disjoint steps.
        let msg: bytes::Bytes = sb.split().freeze();
        encode_ws_frame(&msg, sb);
        let frame: bytes::Bytes = sb.split().freeze();
        for client in clients.values_mut() {
            if matches!(client.state, WsState::Connected) {
                client.send_queue.extend_from_slice(&frame);
            }
        }
        Ok(())
    }

    fn flush(&mut self) -> io::Result<()> {
        for client in self.clients.values_mut() {
            if !client.send_queue.is_empty() {
                match client.stream.write(&client.send_queue) {
                    Ok(n) => {
                        client.send_queue.advance(n);
                    }
                    Err(_) => {}
                }
            }
        }
        Ok(())
    }

    fn ccu(&self) -> usize {
        self.clients
            .values()
            .filter(|c| matches!(c.state, WsState::Connected))
            .count()
    }

    fn local_addr(&self) -> io::Result<SocketAddr> {
        Ok(self.bound_addr)
    }

    fn transport_name(&self) -> &'static str {
        "Colyseus WebSocket (SDK Compatible)"
    }

    fn disconnect(&mut self, client_id: &str) -> io::Result<()> {
        ColyseusWsTransport::disconnect(self, client_id)
    }
}

// -----------------------------------------------------------------------------
// Colyseus frame processing
// -----------------------------------------------------------------------------

/// Decode and dispatch all complete WebSocket frames in the client's receive
/// buffer. Returns `true` when the client must be disconnected (LEAVE_ROOM or
/// a client-initiated WS close). Frames are taken one at a time straight off
/// the buffer — no intermediate frame list.
fn process_ws_frames(
    client: &mut ColyseusClient,
    events_out: &mut Vec<TransportEvent>,
    now: Instant,
) -> bool {
    while let Some((ws_opcode, fin, payload)) = next_ws_frame(&mut client.recv_buf) {
        // Any inbound frame proves the client is alive: refresh the heartbeat
        // deadline (some WebSocket stacks answer app traffic but delay pongs).
        client.last_activity = now;
        client.ping_sent_at = None;
        client.missed_pings = 0;

        match ws_opcode {
            0x8 => {
                // Client-initiated close: echo a close frame, then drop.
                queue_close_frame(client, close_code::CONSENTED, "leave");
                return true;
            }
            0x9 => {
                // RFC 6455 ping -> pong with the same payload.
                queue_ws_control(client, 0xA, &payload);
                continue;
            }
            0xA => {
                // Pong: heartbeat already cleared above.
                continue;
            }
            0x0 => {
                // Continuation fragment.
                if client.frag_opcode.is_some() {
                    client.frag_buf.extend_from_slice(&payload);
                    if fin {
                        let msg = client.frag_buf.split().freeze();
                        client.frag_opcode = None;
                        if dispatch_colyseus_message(client, msg, events_out) {
                            return true;
                        }
                    }
                }
                continue;
            }
            0x1 | 0x2 => {
                if fin {
                    if dispatch_colyseus_message(client, payload, events_out) {
                        return true;
                    }
                } else {
                    client.frag_opcode = Some(ws_opcode);
                    client.frag_buf.clear();
                    client.frag_buf.extend_from_slice(&payload);
                }
                continue;
            }
            _ => continue, // unknown WS opcode — ignore
        }
    }
    false
}

/// Dispatch one complete Colyseus protocol message. `msg[0]` is the protocol
/// byte (modifier bits stripped via `PROTOCOL_CODE_MASK` before dispatch).
/// `msg` is a zero-copy view of the socket receive buffer.
/// Returns `true` when the client must be disconnected (LEAVE_ROOM).
fn dispatch_colyseus_message(
    client: &mut ColyseusClient,
    msg: Bytes,
    events_out: &mut Vec<TransportEvent>,
) -> bool {
    if msg.is_empty() {
        return false;
    }
    let code = msg[0] & PROTOCOL_CODE_MASK;

    match code {
        COLYSEUS_JOIN_ROOM => {
            // Client ack of the server-initiated join — nothing to do;
            // the handshake was already sent proactively on upgrade.
        }
        COLYSEUS_LEAVE_ROOM => {
            // Client leave: answer close 4000 CONSENTED, then drop.
            queue_close_frame(client, close_code::CONSENTED, "leave");
            return true;
        }
        COLYSEUS_ROOM_DATA => {
            events_out.push(TransportEvent::Packet {
                client_id: client.session_id.clone(),
                opcode: OpCode::PlayerInput,
                payload: msg.slice(1..),
            });
        }
        COLYSEUS_ROOM_DATA_BYTES => {
            events_out.push(TransportEvent::Packet {
                client_id: client.session_id.clone(),
                opcode: OpCode::RoomDataBytes,
                payload: msg.slice(1..),
            });
        }
        COLYSEUS_PING => {
            // App-level ping: echo the payload back so the client can
            // measure RTT (payload carries the client's sequence number).
            queue_colyseus_message(client, COLYSEUS_PING, &msg[1..]);
        }
        COLYSEUS_ROOM_REQUEST => {
            events_out.push(TransportEvent::Packet {
                client_id: client.session_id.clone(),
                opcode: OpCode::RoomRequest,
                payload: msg.slice(1..),
            });
        }
        _ => {
            // Unknown/defensive: surface the full frame to the game layer.
            events_out.push(TransportEvent::Packet {
                client_id: client.session_id.clone(),
                opcode: OpCode::Custom,
                payload: msg,
            });
        }
    }
    false
}

/// Map engine-level opcodes onto Colyseus protocol codes.
fn colyseus_opcode_for(opcode: OpCode) -> u8 {
    match opcode {
        OpCode::StateDelta => COLYSEUS_ROOM_STATE_PATCH,
        OpCode::StateSnapshot => COLYSEUS_ROOM_STATE,
        OpCode::RoomDataBytes => COLYSEUS_ROOM_DATA_BYTES,
        OpCode::RoomResponse => COLYSEUS_ROOM_RESPONSE,
        OpCode::Error => COLYSEUS_ERROR,
        OpCode::Ping => COLYSEUS_PING,
        _ => COLYSEUS_ROOM_DATA,
    }
}

// -----------------------------------------------------------------------------
// Colyseus SDK greeting: WS upgrade OR matchmake HTTP seat reservation
// -----------------------------------------------------------------------------

/// Handle a freshly connected client's first bytes. The Colyseus SDK always
/// POSTs `/matchmake/<method>/<room>` first (plain HTTP); only after the
/// seat-reservation JSON does it open the WebSocket connection.
/// Returns `true` when the connection must be closed (HTTP path).
fn handle_client_greeting(
    client: &mut ColyseusClient,
    peer: &str,
    events_out: &mut Vec<TransportEvent>,
    ctx: &mut ServerCtx<'_>,
) -> bool {
    // Extract everything needed from the request up-front (owned), so the
    // borrow of recv_buf ends before we mutate the client.
    let req = String::from_utf8_lossy(&client.recv_buf);
    let is_options = req.starts_with("OPTIONS");
    let is_matchmake = req.contains("/matchmake/") && !req.contains("Sec-WebSocket-Key");
    let room_name = if is_matchmake {
        req.split("/matchmake/")
            .nth(1)
            .and_then(|rest| rest.split('/').nth(1))
            .and_then(|name| name.split_whitespace().next())
            .unwrap_or("game_room")
            .to_string()
    } else {
        String::new()
    };
    let origin = req
        .lines()
        .find(|l| l.to_ascii_lowercase().starts_with("origin:"))
        .and_then(|l| l.split_once(':').map(|(_, v)| v.trim().to_string()))
        .unwrap_or_else(|| "*".to_string());
    let url_session_id = {
        let path = req.split_whitespace().nth(1).unwrap_or("");
        path.split('?')
            .nth(1)
            .and_then(|query| {
                query
                    .split('&')
                    .find_map(|kv| kv.strip_prefix("sessionId="))
            })
            .map(str::to_string)
    };
    // URL path segments (after the leading '/'): matchmake =
    // [matchmake, method, room]; ws = [processId, roomId].
    let url_method = req
        .lines()
        .next()
        .and_then(|l| l.split_whitespace().nth(1))
        .and_then(|p| p.split('/').nth(2).map(str::to_string))
        .unwrap_or_default();
    let url_room = {
        let path = req.split_whitespace().nth(1).unwrap_or("");
        let query = path
            .split_once('?')
            .map(|(_, q)| format!("?{}", q))
            .unwrap_or_default();
        let clean = path.split('?').next().unwrap_or("").trim_matches('/');
        let segments: Vec<&str> = clean.split('/').filter(|s| !s.is_empty()).collect();
        let base_room = if segments.is_empty() {
            ""
        } else if segments.len() == 1 {
            segments[0]
        } else {
            segments.last().unwrap_or(&"")
        };
        if base_room.is_empty() {
            String::new()
        } else {
            format!("{}{}", base_room, query)
        }
    };
    let url_body = req.split("\r\n\r\n").nth(1).unwrap_or("").to_string();
    let is_head = req.starts_with("HEAD ");
    let is_plain_get = (req.starts_with("GET ") || is_head) && !req.contains("Sec-WebSocket-Key");
    let get_path = if is_plain_get {
        req.split_whitespace()
            .nth(1)
            .unwrap_or("/")
            .split('?')
            .next()
            .unwrap_or("/")
            .to_string()
    } else {
        String::new()
    };
    drop(req);

    // CORS: the SDK sends requests with credentials mode 'include', so the
    // wildcard origin is rejected — echo the request's exact Origin instead.
    let cors_allow = format!("Access-Control-Allow-Origin: {}\r\n", origin);

    // --- Matchmake seat reservation (plain HTTP, closed after reply) ---
    // Browsers run the game page on a different origin, so CORS headers are
    // mandatory for the SDK's fetch() to succeed (incl. OPTIONS preflight).
    if is_options {
        let response = format!(
            "HTTP/1.1 204 No Content\r\n\
             {}\
             Access-Control-Allow-Credentials: true\r\n\
             Access-Control-Allow-Methods: POST, OPTIONS\r\n\
             Access-Control-Allow-Headers: Content-Type, Accept\r\n\
             Access-Control-Allow-Private-Network: true\r\n\
             Access-Control-Max-Age: 86400\r\n\
             Connection: close\r\n\r\n",
            cors_allow
        );
        let _ = client.stream.write_all(response.as_bytes());
        client.recv_buf.clear();
        return true;
    }

    if is_matchmake {
        // Game hook decides: create/join/joinById/reconnect or reject with a
        // typed MatchMakeError. Without a hook, the transport serves the
        // standard Colyseus method whitelist itself.
        let result = match ctx.matchmake_hook {
            Some(hook) => hook(&url_method, &room_name, &url_body),
            None => default_matchmake(
                &url_method,
                &room_name,
                &url_body,
                ctx.retained,
                &client.session_id,
            ),
        };
        let (status, payload) = match result {
            Ok(seat) => {
                // Remember the seat's reconnectionToken so the JOIN_ROOM
                // handshake advertises the same token the SDK reconnects with.
                if let (Some(sid), Some(token)) = (
                    parse_json_string(&seat, "sessionId"),
                    parse_json_string(&seat, "reconnectionToken"),
                ) {
                    ctx.seat_tokens.insert(sid, token);
                }
                ("200 OK", seat)
            }
            Err(err) => (
                "404 Not Found",
                format!(
                    "{{\"code\":{},\"error\":\"{}\"}}",
                    err.code,
                    err.message.replace('"', "'")
                ),
            ),
        };
        let response = format!(
            "HTTP/1.1 {}\r\nContent-Type: application/json\r\n{}Access-Control-Allow-Credentials: true\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            status,
            cors_allow,
            payload.len(),
            payload
        );
        let _ = client.stream.write_all(response.as_bytes());
        client.recv_buf.clear();
        return true; // close after HTTP reply
    }

    // --- Static HTTP requests (if hook registered and not a WS upgrade) ---
    if is_plain_get {
        if let Some(hook) = ctx.http_hook {
            if let Some((content_type, body)) = hook(&get_path) {
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: {}\r\nContent-Length: {}\r\nAccess-Control-Allow-Origin: *\r\nAccess-Control-Allow-Methods: GET, OPTIONS\r\nCache-Control: no-cache, no-store, must-revalidate\r\nPragma: no-cache\r\nExpires: 0\r\nConnection: close\r\n\r\n",
                    content_type,
                    body.len()
                );
                let sock = socket2::SockRef::from(&client.stream);
                let _ = sock.set_nonblocking(false);
                let _ = (&client.stream).write_all(response.as_bytes());
                if !is_head {
                    let _ = (&client.stream).write_all(&body);
                }
                let _ = client.stream.flush();
                client.recv_buf.clear();
                return true;
            }
        }
    }

    // --- RFC 6455 WebSocket upgrade ---
    if let Some(resp) = handle_ws_handshake(&client.recv_buf) {
        let _ = client.stream.write_all(&resp);
        client.recv_buf.clear();
        client.state = WsState::Connected;
        client.last_activity = Instant::now();

        // Adopt the SDK's seat-reservation sessionId from the upgrade URL so
        // the server-side player key matches room.sessionId on the client.
        if let Some(sid) = &url_session_id {
            client.session_id = Arc::from(sid.as_str());
        }
        // Remember which room this socket belongs to (for the game layer).
        client.room_id = url_room.clone();
        if !url_room.is_empty() {
            ctx.client_rooms
                .insert(client.session_id.to_string(), url_room.clone());
        }

        // In this protocol the SERVER initiates the join: the SDK client
        // connects, then waits for the JOIN_ROOM handshake before `onJoin`.
        let reply = build_join_reply(&client.session_id, &client.room_id, ctx.seat_tokens);
        encode_ws_frame(&reply, &mut client.send_queue);

        if let Ok(peer_addr) = client.stream.peer_addr() {
            events_out.push(TransportEvent::Connected {
                client_id: client.session_id.clone(),
                peer_addr,
            });
        } else if let Some(addr) = peer.parse().ok() {
            events_out.push(TransportEvent::Connected {
                client_id: client.session_id.clone(),
                peer_addr: addr,
            });
        }
    }
    false
}

/// Default matchmake served when the game has not registered a hook: seats for
/// the full Colyseus method whitelist, reconnect via retained tokens, and
/// `MATCHMAKE_NO_HANDLER` for anything else.
fn default_matchmake(
    method: &str,
    room: &str,
    body: &str,
    retained: &HashMap<String, String>,
    fallback_session: &str,
) -> Result<String, MatchMakeError> {
    if method == "reconnect" {
        let token = parse_json_string(body, "reconnectionToken").unwrap_or_default();
        if retained.contains_key(&token) {
            let session = token.split(':').nth(1).unwrap_or(fallback_session);
            return Ok(seat_json(room, session, &token));
        }
        return Err(MatchMakeError::expired(
            "reconnection token expired or unknown",
        ));
    }

    if !MATCHMAKE_METHODS.contains(&method) {
        return Err(MatchMakeError::no_handler(format!(
            "no room handler for method '{}'",
            method
        )));
    }

    let session = format!("sdk_{}", now_millis());
    let token = format!("{}:{}", room, session);
    Ok(seat_json(room, &session, &token))
}

/// `ISeatReservation` JSON consumed by the SDK (`packages/sdk/src/Client.ts`).
fn seat_json(room: &str, session_id: &str, token: &str) -> String {
    format!(
        "{{\"name\":\"{room}\",\"sessionId\":\"{sid}\",\"roomId\":\"{room}\",\"processId\":\"openoxide_1\",\"reconnectionToken\":\"{token}\"}}",
        room = room,
        sid = session_id,
        token = token
    )
}

fn now_millis() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0)
}

/// Minimal JSON string-field scanner for seat-reservation bodies
/// (e.g. `{"reconnectionToken":"game_room:sess_1"}`).
fn parse_json_string(body: &str, key: &str) -> Option<String> {
    let needle = format!("\"{}\"", key);
    let idx = body.find(&needle)?;
    let rest = body[idx + needle.len()..].strip_prefix(':')?.trim_start();
    let rest = rest.strip_prefix('"')?;
    let end = rest.find('"')?;
    Some(rest[..end].to_string())
}

/// Build the SDK-format JOIN_ROOM handshake frame:
/// [10][rt-len][rt][serializer-len]["schema"][msgpack 0 = empty state reflection]
fn build_join_reply(
    session_id: &str,
    room_id: &str,
    seat_tokens: &HashMap<String, String>,
) -> Vec<u8> {
    let rt = seat_tokens
        .get(session_id)
        .cloned()
        .unwrap_or_else(|| format!("{}:{}", room_id, session_id));
    let mut reply = Vec::with_capacity(1 + 1 + rt.len() + 1 + SERIALIZER_ID.len() + 1);
    reply.push(COLYSEUS_JOIN_ROOM);
    reply.push(rt.len() as u8);
    reply.extend_from_slice(rt.as_bytes());
    reply.push(SERIALIZER_ID.len() as u8);
    reply.extend_from_slice(SERIALIZER_ID);
    reply.push(0x00); // msgpack fixint 0: empty state reflection
    reply
}

// -----------------------------------------------------------------------------
// Pure-Rust RFC 6455 WebSocket Handshake & Frame Utilities (Zero Dependencies)
// -----------------------------------------------------------------------------

fn handle_ws_handshake(buf: &[u8]) -> Option<Vec<u8>> {
    let req = String::from_utf8_lossy(buf);
    if !req.contains("\r\n\r\n") {
        return None;
    }

    for line in req.lines() {
        if line.to_lowercase().starts_with("sec-websocket-key:") {
            let key = line.split(':').nth(1)?.trim();
            let accept_val = compute_ws_accept(key);
            let response = format!(
                "HTTP/1.1 101 Switching Protocols\r\n\
                 Upgrade: websocket\r\n\
                 Connection: Upgrade\r\n\
                 Sec-WebSocket-Accept: {}\r\n\r\n",
                accept_val
            );
            return Some(response.into_bytes());
        }
    }
    None
}

/// Take one frame off the front of `buf`. `None` when the buffer holds an
/// incomplete frame. Unmasks the payload in place and hands out a refcounted
/// `Bytes` view — no per-frame copy.
fn next_ws_frame(buf: &mut BytesMut) -> Option<(u8, bool, Bytes)> {
    if buf.len() < 2 {
        return None;
    }

    let b0 = buf[0];
    let ws_opcode = b0 & 0x0F;
    let fin = (b0 & 0x80) != 0;
    let b1 = buf[1];
    let is_masked = (b1 & 0x80) != 0;
    let mut payload_len = (b1 & 0x7F) as usize;
    let mut header_len = 2;

    if payload_len == 126 {
        if buf.len() < 4 {
            return None;
        }
        payload_len = u16::from_be_bytes([buf[2], buf[3]]) as usize;
        header_len = 4;
    } else if payload_len == 127 {
        if buf.len() < 10 {
            return None;
        }
        payload_len = u64::from_be_bytes(buf[2..10].try_into().unwrap()) as usize;
        header_len = 10;
    }

    let mask_len = if is_masked { 4 } else { 0 };
    let total_len = header_len + mask_len + payload_len;

    if buf.len() < total_len {
        return None;
    }

    // Detach the whole frame, then unmask the payload region in place and
    // hand out a zero-copy view of it.
    let mut frame = buf.split_to(total_len);
    let payload_start = header_len + mask_len;
    if is_masked {
        let mask = [
            frame[header_len],
            frame[header_len + 1],
            frame[header_len + 2],
            frame[header_len + 3],
        ];
        for (i, byte) in frame[payload_start..total_len].iter_mut().enumerate() {
            *byte ^= mask[i & 3];
        }
    }
    let payload = frame.split_off(payload_start).freeze();
    Some((ws_opcode, fin, payload))
}

/// Server frames are never masked (RFC 6455).
fn encode_ws_frame(payload: &[u8], out: &mut impl BufMut) {
    // Binary Frame: FIN bit (0x80) + Opcode Binary (0x02) = 0x82
    out.put_u8(0x82);

    let len = payload.len();
    if len <= 125 {
        out.put_u8(len as u8);
    } else if len <= 65535 {
        out.put_u8(126);
        out.put_slice(&(len as u16).to_be_bytes());
    } else {
        out.put_u8(127);
        out.put_slice(&(len as u64).to_be_bytes());
    }

    out.put_slice(payload);
}

/// Queue one Colyseus protocol message (opcode byte + payload) as a WS frame.
fn queue_colyseus_message(client: &mut ColyseusClient, code: u8, payload: &[u8]) {
    encode_colyseus_frame(code, payload, &mut client.send_queue);
}

/// Encode `[code][payload]` as an unmasked binary WS frame directly into
/// `out`, with no scratch buffer.
fn encode_colyseus_frame(code: u8, payload: &[u8], out: &mut impl BufMut) {
    let len = payload.len() + 1;
    out.put_u8(0x82);
    if len <= 125 {
        out.put_u8(len as u8);
    } else if len <= 65535 {
        out.put_u8(126);
        out.put_slice(&(len as u16).to_be_bytes());
    } else {
        out.put_u8(127);
        out.put_slice(&(len as u64).to_be_bytes());
    }
    out.put_u8(code);
    out.put_slice(payload);
}

/// Queue a WS control frame (ping/pong/close — always FIN, payload <= 125).
fn queue_ws_control(client: &mut ColyseusClient, opcode: u8, payload: &[u8]) {
    client.send_queue.put_u8(0x80 | opcode);
    client.send_queue.put_u8(payload.len().min(125) as u8);
    client
        .send_queue
        .put_slice(&payload[..payload.len().min(125)]);
}

/// Queue a WS ping (heartbeat).
fn send_ws_ping(client: &mut ColyseusClient) {
    queue_ws_control(client, 0x89, &[]);
    try_flush_client(client);
}

/// Queue a WS close frame with a Colyseus close code + reason.
fn queue_close_frame(client: &mut ColyseusClient, code: u16, reason: &str) {
    let mut payload = Vec::with_capacity(2 + reason.len());
    payload.extend_from_slice(&code.to_be_bytes());
    payload.extend_from_slice(&reason.as_bytes()[..reason.len().min(123)]);
    queue_ws_control(client, 0x8, &payload);
}

/// Best-effort immediate write of queued bytes (non-blocking).
fn try_flush_client(client: &mut ColyseusClient) {
    if client.send_queue.is_empty() {
        return;
    }
    match client.stream.write(&client.send_queue) {
        Ok(n) => {
            client.send_queue.advance(n);
        }
        Err(_) => {}
    }
}

/// Final write for a dying client: switch the socket to blocking so any queued
/// close frame is guaranteed on the wire, then drop (which closes the fd).
fn finish_close(client: ColyseusClient) {
    let client = client;
    if !client.send_queue.is_empty() {
        let std_stream: std::net::TcpStream = client.stream.into();
        let _ = std_stream.set_nonblocking(false);
        let _ = (&std_stream).write_all(&client.send_queue);
        // std_stream drops here, closing the socket.
    }
}

// RFC 3174 SHA-1 & Base64 (Self-contained, ultra fast)
fn compute_ws_accept(key: &str) -> String {
    const MAGIC: &str = "258EAFA5-E914-47DA-95CA-C5AB0DC85B11";
    let concatenated = format!("{}{}", key, MAGIC);
    let digest = sha1(concatenated.as_bytes());
    base64_encode(&digest)
}

fn sha1(data: &[u8]) -> [u8; 20] {
    let mut h = [
        0x67452301u32,
        0xEFCDAB89u32,
        0x98BADCFEu32,
        0x10325476u32,
        0xC3D2E1F0u32,
    ];

    let mut msg = data.to_vec();
    let bit_len = (data.len() as u64) * 8;
    msg.push(0x80);
    while (msg.len() * 8) % 512 != 448 {
        msg.push(0x00);
    }
    msg.extend_from_slice(&bit_len.to_be_bytes());

    for chunk in msg.chunks_exact(64) {
        let mut w = [0u32; 80];
        for i in 0..16 {
            w[i] = u32::from_be_bytes(chunk[i * 4..(i + 1) * 4].try_into().unwrap());
        }
        for i in 16..80 {
            w[i] = (w[i - 3] ^ w[i - 8] ^ w[i - 14] ^ w[i - 16]).rotate_left(1);
        }

        let (mut a, mut b, mut c, mut d, mut e) = (h[0], h[1], h[2], h[3], h[4]);
        for i in 0..80 {
            let (f, k) = match i {
                0..=19 => ((b & c) | (!b & d), 0x5A827999),
                20..=39 => (b ^ c ^ d, 0x6ED9EBA1),
                40..=59 => ((b & c) | (b & d) | (c & d), 0x8F1BBCDC),
                _ => (b ^ c ^ d, 0xCA62C1D6),
            };
            let temp = a
                .rotate_left(5)
                .wrapping_add(f)
                .wrapping_add(e)
                .wrapping_add(k)
                .wrapping_add(w[i]);
            e = d;
            d = c;
            c = b.rotate_left(30);
            b = a;
            a = temp;
        }

        h[0] = h[0].wrapping_add(a);
        h[1] = h[1].wrapping_add(b);
        h[2] = h[2].wrapping_add(c);
        h[3] = h[3].wrapping_add(d);
        h[4] = h[4].wrapping_add(e);
    }

    let mut out = [0u8; 20];
    for (i, val) in h.iter().enumerate() {
        out[i * 4..(i + 1) * 4].copy_from_slice(&val.to_be_bytes());
    }
    out
}

fn base64_encode(input: &[u8]) -> String {
    const CHARSET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in input.chunks(3) {
        let b0 = chunk[0];
        let b1 = if chunk.len() > 1 { chunk[1] } else { 0 };
        let b2 = if chunk.len() > 2 { chunk[2] } else { 0 };

        let n = ((b0 as u32) << 16) | ((b1 as u32) << 8) | (b2 as u32);
        out.push(CHARSET[((n >> 18) & 63) as usize] as char);
        out.push(CHARSET[((n >> 12) & 63) as usize] as char);
        if chunk.len() > 1 {
            out.push(CHARSET[((n >> 6) & 63) as usize] as char);
        } else {
            out.push('=');
        }
        if chunk.len() > 2 {
            out.push(CHARSET[(n & 63) as usize] as char);
        } else {
            out.push('=');
        }
    }
    out
}
