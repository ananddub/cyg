//! # Raw mio WebSocket Server
//!
//! Thread-per-core friendly, no async runtime — the same philosophy as
//! engine-net's `GameTransport`: `poll_events(timeout, &mut buf)` drains the epoll
//! batch, handlers get owned events, writes are queued per connection and
//! flushed on `WRITABLE`.
//!
//! Hot paths avoid per-event allocation: session ids are `Arc<str>` (cloning
//! is a refcount bump), payloads ride zero-copy `Bytes` views, the write queue
//! is a `BytesMut` drained with O(1) `advance`, and every poll-scratch buffer
//! lives on the server or connection across polls.
//!
//! State machine per socket: `Handshaking -> Open -> CloseSent -> (removed)`.
//! Ping is auto-answered with Pong; Close echoes the close frame once.

use std::collections::HashMap;
use std::io::{self, Read, Write};
use std::net::{SocketAddr, TcpListener as StdTcpListener};
use std::sync::Arc;
use std::time::Duration;

use bytes::{Buf, BufMut, Bytes, BytesMut};
use mio::net::TcpStream;
use mio::{Events, Interest, Poll, Token, Waker};
use socket2::{Domain, Protocol, Socket, Type};

use crate::frame::{
    Frame, FrameDecoder, FrameError, close_code, close_payload, encode_frame, opcode, parse_close,
};
use crate::handshake::{HandshakeOutcome, HandshakeReader, accept_response, reject_response};

const LISTENER: Token = Token(0);
const WAKER: Token = Token(1);

/// Events surfaced to the game/engine side of the wire.
///
/// `client_id` is an `Arc<str>`: handing it to handlers or storing it in maps
/// clones a refcount, never the string.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WsEvent {
    Connected {
        client_id: Arc<str>,
        peer_addr: SocketAddr,
        path: String,
    },
    Disconnected {
        client_id: Arc<str>,
        code: u16,
    },
    /// TEXT payload, UTF-8 validated in place at decode time. The bytes are a
    /// zero-copy `Bytes` view; consumers wanting `&str` call
    /// `std::str::from_utf8(&text)`.
    Text {
        client_id: Arc<str>,
        text: Bytes,
    },
    Binary {
        client_id: Arc<str>,
        data: Bytes,
    },
    /// A Pong reply to a server-initiated Ping (RTT probe).
    Pong {
        client_id: Arc<str>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ConnState {
    Handshaking,
    Open,
    CloseSent,
}

/// Outcome of feeding bytes to a connection.
enum Step {
    KeepReading,
    Upgraded,
    Rejected,
    Drained,
    /// Frames were decoded into the connection's persistent scratch buffer.
    FramesReady,
    ProtocolViolation(u16),
}

struct WsConn {
    socket: TcpStream,
    peer_addr: SocketAddr,
    client_id: Arc<str>,
    state: ConnState,
    handshake: HandshakeReader,
    path: String,
    reader: FrameDecoder,
    /// Cross-read frame scratch; capacity persists on the connection.
    frames: Vec<Frame>,
    frag_buf: BytesMut,
    frag_opcode: u8,
    /// Outgoing bytes as a contiguous `BytesMut`: enqueue is one memcpy and
    /// the written prefix is detached in O(1) (`Buf::advance`).
    write_queue: BytesMut,
    write_wanted: bool,
    /// After flushing a rejection/101 we may need to drop the socket.
    drop_after_flush: bool,
}

impl WsConn {
    fn queue(&mut self, bytes: &[u8]) {
        self.write_queue.reserve(bytes.len());
        self.write_queue.put_slice(bytes);
    }
}

/// Configuration limits for one server.
#[derive(Debug, Clone, Copy)]
pub struct WsConfig {
    /// Maximum buffered bytes per connection in the handshake phase.
    pub max_handshake_bytes: usize,
    /// Maximum single-frame payload accepted from clients.
    pub max_frame_bytes: usize,
    /// Maximum queued outgoing bytes before a slow client is dropped.
    pub max_write_queue_bytes: usize,
}

impl Default for WsConfig {
    fn default() -> Self {
        Self {
            max_handshake_bytes: 16 * 1024,
            max_frame_bytes: 256 * 1024,
            max_write_queue_bytes: 10 * 1024 * 1024,
        }
    }
}

/// Blocking-free WebSocket server on one `SO_REUSEPORT` TCP listener.
pub struct WsServer {
    poll: Poll,
    waker: Waker,
    listener: mio::net::TcpListener,
    conns: HashMap<usize, WsConn>,
    by_client: HashMap<String, usize>,
    next_client_id: usize,
    next_token: usize,
    events: Events,
    config: WsConfig,
    /// Cross-poll event buffer; adopted via `mem::take` so helpers taking
    /// `&mut self` can append, then handed to the caller.
    event_buf: Vec<WsEvent>,
    /// Ready token list; adopted and restored each poll so its capacity
    /// survives — `mem::take` + drop would re-malloc every poll.
    ready_tokens: Vec<Token>,
    /// Writable-only token list; adopted and restored each poll.
    ready_writes: Vec<Token>,
}

impl WsServer {
    /// Bind with `SO_REUSEPORT` so N threads can share one port (thread-per-core).
    pub fn new(bind_addr: SocketAddr, config: WsConfig) -> io::Result<Self> {
        let sock = Socket::new(Domain::for_address(bind_addr), Type::STREAM, Some(Protocol::TCP))
            .map_err(io::Error::other)?;
        sock.set_reuse_address(true).ok();
        let _ = sock.set_reuse_port(true);
        sock.set_nonblocking(true)?;
        sock.bind(&bind_addr.into())
            .map_err(|e| io::Error::new(e.kind(), format!("bind {bind_addr}: {e}")))?;
        sock.listen(1024)?;
        let std: StdTcpListener = sock.into();
        let mut listener = mio::net::TcpListener::from_std(std);

        let poll = Poll::new()?;
        let waker = Waker::new(poll.registry(), WAKER)?;
        poll.registry()
            .register(&mut listener, LISTENER, Interest::READABLE)?;

        Ok(Self {
            poll,
            waker,
            listener,
            conns: HashMap::new(),
            by_client: HashMap::new(),
            next_client_id: 0,
            next_token: 2,
            events: Events::with_capacity(1024),
            config,
            event_buf: Vec::new(),
            ready_tokens: Vec::new(),
            ready_writes: Vec::new(),
        })
    }

    pub fn local_addr(&self) -> io::Result<SocketAddr> {
        self.listener.local_addr()
    }

    /// Open (post-handshake) connections.
    pub fn ccu(&self) -> usize {
        self.conns.values().filter(|c| c.state == ConnState::Open).count()
    }

    /// Drive the event loop once, appending all engine-visible events to the
    /// caller-owned `out` buffer (reused across polls — no per-poll allocation).
    pub fn poll_events(&mut self, timeout: Duration, out: &mut Vec<WsEvent>) -> io::Result<()> {
        self.poll.poll(&mut self.events, Some(timeout))?;

        // Split borrows: `events` is read immutably while the persistent
        // token buffers are written — no per-poll Vec allocations.
        let mut have_listener = false;
        {
            let (ready, writes) = (&mut self.ready_tokens, &mut self.ready_writes);
            ready.clear();
            writes.clear();
            for ev in self.events.iter() {
                match ev.token() {
                    LISTENER => have_listener = true,
                    WAKER => {},
                    t => {
                        if ev.is_readable() {
                            ready.push(t);
                        }
                        if ev.is_writable() {
                            writes.push(t);
                        }
                    },
                }
            }
        }

        // Helpers below take `&mut self`, so the persistent buffers are
        // adopted for the pass and restored afterwards (capacity survives).
        let mut buf = std::mem::take(&mut self.event_buf);
        let mut writes = std::mem::take(&mut self.ready_writes);
        let mut reads = std::mem::take(&mut self.ready_tokens);
        if have_listener {
            self.accept_all(&mut buf);
        }
        for i in 0..writes.len() {
            let token = writes[i];
            self.flush(token);
        }
        writes.clear();
        self.ready_writes = writes;
        for i in 0..reads.len() {
            let token = reads[i];
            self.on_readable(token, &mut buf);
        }
        reads.clear();
        self.ready_tokens = reads;
        std::mem::swap(out, &mut buf);
        buf.clear();
        self.event_buf = buf;
        Ok(())
    }

    /// Wake the blocking `poll` from another thread (shutdown/notify hook).
    pub fn waker(&self) -> &Waker {
        &self.waker
    }

    // ---- outgoing API ---------------------------------------------------

    pub fn send_text(&mut self, client_id: &str, text: &str) -> io::Result<()> {
        self.send_frame(client_id, frame_text(text.as_bytes()))
    }

    pub fn send_binary(&mut self, client_id: &str, data: &[u8]) -> io::Result<()> {
        self.send_frame(client_id, frame_binary(data))
    }

    pub fn ping(&mut self, client_id: &str, data: &[u8]) -> io::Result<()> {
        self.send_frame(client_id, frame_ping(data))
    }

    /// Server-initiated graceful close.
    pub fn close(&mut self, client_id: &str, code: u16, reason: &str) -> io::Result<()> {
        let token = self.token_of(client_id).ok_or_else(not_connected)?;
        let frame = frame_close(code, reason);
        if let Some(conn) = self.conns.get_mut(&token.0) {
            conn.queue(&frame);
            conn.state = ConnState::CloseSent;
        }
        self.flush(token);
        Ok(())
    }

    pub fn broadcast_text(&mut self, text: &str) {
        self.broadcast(frame_text(text.as_bytes()));
    }

    pub fn broadcast_binary(&mut self, data: &[u8]) {
        self.broadcast(frame_binary(data));
    }

    /// Queue one frame to every open connection. The frame is a `Bytes` (one
    /// buffer shared by all, queued by memcpy into each write queue), and the
    /// numeric token list avoids cloning any client ids.
    fn broadcast(&mut self, frame: Bytes) {
        let mut tokens: Vec<usize> = self
            .conns
            .iter()
            .filter(|(_, c)| c.state == ConnState::Open)
            .map(|(k, _)| *k)
            .collect();
        tokens.sort_unstable();
        for t in tokens {
            let overflow = {
                let Some(conn) = self.conns.get_mut(&t) else { continue };
                conn.queue(&frame);
                conn.write_queue.len() > self.config.max_write_queue_bytes
            };
            if overflow {
                let mut scratch = Vec::new();
                self.drop_conn(Token(t), close_code::POLICY_VIOLATION, &mut scratch);
                continue;
            }
            self.flush(Token(t));
        }
    }

    fn send_frame(&mut self, client_id: &str, frame: Bytes) -> io::Result<()> {
        let token = self.token_of(client_id).ok_or_else(not_connected)?;
        let overflow = {
            let Some(conn) = self.conns.get_mut(&token.0) else {
                return Err(not_connected());
            };
            if conn.state != ConnState::Open {
                return Err(io::Error::new(io::ErrorKind::NotConnected, "ws not open"));
            }
            conn.queue(&frame);
            conn.write_queue.len() > self.config.max_write_queue_bytes
        };
        if overflow {
            let mut scratch = Vec::new();
            self.drop_conn(token, close_code::POLICY_VIOLATION, &mut scratch);
            return Err(io::Error::new(
                io::ErrorKind::WouldBlock,
                "client write buffer overflow",
            ));
        }
        self.flush(token);
        Ok(())
    }

    fn token_of(&self, client_id: &str) -> Option<Token> {
        self.by_client.get(client_id).map(|t| Token(*t))
    }

    // ---- internals ------------------------------------------------------

    fn accept_all(&mut self, out: &mut Vec<WsEvent>) {
        loop {
            match self.listener.accept() {
                Ok((mut socket, peer_addr)) => {
                    let token = Token(self.next_token);
                    self.next_token += 1;
                    if self
                        .poll
                        .registry()
                        .register(&mut socket, token, Interest::READABLE)
                        .is_err()
                    {
                        continue;
                    }
                    let client_id: Arc<str> = format!("ws-{}", self.next_client_id).into();
                    self.next_client_id += 1;
                    let conn = WsConn {
                        socket,
                        peer_addr,
                        client_id: client_id.clone(),
                        state: ConnState::Handshaking,
                        handshake: HandshakeReader::new(),
                        path: String::new(),
                        reader: FrameDecoder::new(self.config.max_frame_bytes),
                        frames: Vec::new(),
                        frag_buf: BytesMut::new(),
                        frag_opcode: 0,
                        write_queue: BytesMut::new(),
                        write_wanted: false,
                        drop_after_flush: false,
                    };
                    self.by_client.insert(client_id.to_string(), token.0);
                    self.conns.insert(token.0, conn);
                    let _ = out;
                },
                Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => break,
                Err(e) => {
                    eprintln!("ws accept error: {e}");
                    break;
                },
            }
        }
    }

    fn on_readable(&mut self, token: Token, out: &mut Vec<WsEvent>) {
        loop {
            let step = {
                let Some(conn) = self.conns.get_mut(&token.0) else {
                    return;
                };
                if conn.state != ConnState::Handshaking {
                    // Open socket: read straight into the decoder's buffer —
                    // frames are zero-copy views, no intermediate 16 KB copy —
                    // and decode into the connection's persistent scratch.
                    let (reader, frames, socket) =
                        (&mut conn.reader, &mut conn.frames, &mut conn.socket);
                    frames.clear();
                    match reader.feed_socket(socket, 16 * 1024, frames) {
                        Ok((0, _)) => Step::ProtocolViolation(close_code::GOING_AWAY),
                        Ok((_, Ok(()))) => Step::FramesReady,
                        Ok((_, Err(FrameError::Protocol(_)))) => {
                            Step::ProtocolViolation(close_code::PROTOCOL_ERROR)
                        },
                        Ok((_, Err(FrameError::TooBig(_)))) => {
                            Step::ProtocolViolation(close_code::TOO_BIG)
                        },
                        Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => Step::Drained,
                        Err(_) => Step::ProtocolViolation(close_code::GOING_AWAY),
                    }
                } else {
                    let mut buf = [0u8; 16 * 1024];
                    match conn.socket.read(&mut buf) {
                        Ok(0) => Step::ProtocolViolation(close_code::GOING_AWAY),
                        Ok(n) => {
                            if conn.handshake.fed_bytes() + n > self.config.max_handshake_bytes {
                                conn.queue(&reject_response(431, "handshake too large"));
                                conn.drop_after_flush = true;
                                Step::Rejected
                            } else {
                                match conn.handshake.feed(&buf[..n]) {
                                    HandshakeOutcome::NeedMore => Step::KeepReading,
                                    HandshakeOutcome::Ok(req) => {
                                        conn.path = req.path.clone();
                                        conn.queue(&accept_response(&req, &[]));
                                        conn.state = ConnState::Open;
                                        Step::Upgraded
                                    },
                                    HandshakeOutcome::Reject(status, reason) => {
                                        conn.queue(&reject_response(status, reason));
                                        conn.drop_after_flush = true;
                                        Step::Rejected
                                    },
                                }
                            }
                        },
                        Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => return,
                        Err(_) => Step::ProtocolViolation(close_code::GOING_AWAY),
                    }
                }
            };

            match step {
                Step::KeepReading => {},
                Step::Upgraded => {
                    self.flush(token);
                    if let Some(conn) = self.conns.get(&token.0) {
                        out.push(WsEvent::Connected {
                            client_id: conn.client_id.clone(),
                            peer_addr: conn.peer_addr,
                            path: conn.path.clone(),
                        });
                    }
                },
                Step::Rejected => {
                    self.flush(token);
                    if self
                        .conns
                        .get(&token.0)
                        .is_some_and(|c| c.write_queue.is_empty())
                    {
                        self.drop_conn(token, close_code::NORMAL, out);
                    }
                },
                Step::Drained => return,
                Step::FramesReady => {
                    if !self.handle_frames(token, out) {
                        return;
                    }
                    self.flush(token);
                },
                Step::ProtocolViolation(code) => {
                    self.drop_conn(token, code, out);
                    return;
                },
            }
        }
    }

    /// Drain the connection's decoded frames, classifying and dispatching
    /// each. Returns `false` when the connection was removed mid-processing.
    fn handle_frames(&mut self, token: Token, out: &mut Vec<WsEvent>) -> bool {
        loop {
            // Frames live on the conn, so take them one at a time (the batch
            // is tiny; `remove(0)` shifts a handful of 24-byte structs).
            let action = {
                let Some(conn) = self.conns.get_mut(&token.0) else {
                    return false;
                };
                if conn.frames.is_empty() {
                    break;
                }
                let frame = conn.frames.remove(0);
                classify_frame(conn, frame)
            };
            match action {
                FrameAction::None => {},
                FrameAction::Text(id, text) => out.push(WsEvent::Text { client_id: id, text }),
                FrameAction::Binary(id, data) => out.push(WsEvent::Binary { client_id: id, data }),
                FrameAction::Pong(id) => out.push(WsEvent::Pong { client_id: id }),
                FrameAction::SendPong(data) => {
                    let frame = frame_pong(&data);
                    if let Some(conn) = self.conns.get_mut(&token.0) {
                        conn.queue(&frame);
                    }
                },
                FrameAction::CloseEcho(code) => {
                    if let Some(conn) = self.conns.get_mut(&token.0) {
                        if conn.state == ConnState::Open {
                            conn.queue(&frame_close(code, ""));
                            conn.state = ConnState::CloseSent;
                            // RFC 6455 §7.1.1: after replying to a CLOSE,
                            // tear the transport down once the echo drains.
                            conn.drop_after_flush = true;
                            continue;
                        }
                        // Close handshake complete (we already echoed): TCP
                        // teardown after the queued bytes drain.
                        conn.drop_after_flush = true;
                        continue;
                    }
                    return false;
                },
                FrameAction::InvalidUtf8 => {
                    let frame = frame_close(close_code::INVALID_PAYLOAD, "");
                    if let Some(conn) = self.conns.get_mut(&token.0) {
                        conn.queue(&frame);
                        conn.state = ConnState::CloseSent;
                    }
                    self.flush(token);
                    self.drop_conn(token, close_code::INVALID_PAYLOAD, out);
                    return false;
                },
            }
        }
        self.conns.contains_key(&token.0)
    }

    fn drop_conn(&mut self, token: Token, code: u16, out: &mut Vec<WsEvent>) {
        if let Some(conn) = self.conns.remove(&token.0) {
            // `by_client` is keyed by String; borrow the Arc<str> in place.
            self.by_client.remove(&*conn.client_id);
            if conn.state != ConnState::Handshaking {
                // Move the id out — the event owns it, no clone.
                out.push(WsEvent::Disconnected {
                    client_id: conn.client_id,
                    code,
                });
            }
            // Dropping `conn.socket` deregisters it from epoll (mio semantics).
        }
    }

    /// Best-effort write of the queued bytes; registers WRITABLE if blocked.
    fn flush(&mut self, token: Token) {
        let Some(conn) = self.conns.get_mut(&token.0) else {
            return;
        };
        loop {
            if conn.write_queue.is_empty() {
                break;
            }
            let take = conn.write_queue.len().min(64 * 1024);
            match conn.socket.write(&conn.write_queue[..take]) {
                Ok(0) => break,
                Ok(n) => {
                    // O(1) pointer bump — the written prefix is detached
                    // without memmoving the remaining bytes.
                    conn.write_queue.advance(n);
                },
                Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                    if !conn.write_wanted {
                        let _ = self.poll.registry().reregister(
                            &mut conn.socket,
                            token,
                            Interest::READABLE | Interest::WRITABLE,
                        );
                        conn.write_wanted = true;
                    }
                    return;
                },
                Err(_) => break,
            }
        }
        if conn.write_queue.is_empty() && conn.write_wanted {
            let _ = self
                .poll
                .registry()
                .reregister(&mut conn.socket, token, Interest::READABLE);
            conn.write_wanted = false;
        }
        if conn.drop_after_flush && conn.write_queue.is_empty() {
            let mut scratch = Vec::new();
            self.drop_conn(token, close_code::NORMAL, &mut scratch);
        }
    }
}

fn not_connected() -> io::Error {
    io::Error::new(io::ErrorKind::NotConnected, "ws client not connected")
}

enum FrameAction {
    None,
    Text(Arc<str>, Bytes),
    Binary(Arc<str>, Bytes),
    Pong(Arc<str>),
    SendPong(Bytes),
    CloseEcho(u16),
    InvalidUtf8,
}

fn classify_frame(conn: &mut WsConn, frame: Frame) -> FrameAction {
    let Frame { fin, opcode: op, payload } = frame;

    // Once we've answered a CLOSE, swallow everything except the final CLOSE.
    if conn.state == ConnState::CloseSent {
        return if op == opcode::CLOSE {
            FrameAction::CloseEcho(close_code::NORMAL)
        } else {
            FrameAction::None
        };
    }

    match op {
        opcode::TEXT | opcode::BINARY => {
            if !fin {
                conn.frag_buf.extend_from_slice(&payload);
                conn.frag_opcode = op;
                return FrameAction::None;
            }
            let (final_op, data) = if conn.frag_buf.is_empty() {
                (op, payload)
            } else {
                conn.frag_buf.extend_from_slice(&payload);
                (conn.frag_opcode, conn.frag_buf.split().freeze())
            };
            deliver(conn, final_op, data)
        },
        opcode::CONTINUATION => {
            conn.frag_buf.extend_from_slice(&payload);
            if !fin {
                return FrameAction::None;
            }
            let op = conn.frag_opcode;
            let data = conn.frag_buf.split().freeze();
            deliver(conn, op, data)
        },
        opcode::PING => FrameAction::SendPong(payload),
        opcode::PONG => FrameAction::Pong(conn.client_id.clone()),
        opcode::CLOSE => {
            let (code, _) = parse_close(&payload);
            FrameAction::CloseEcho(code)
        },
        _ => FrameAction::CloseEcho(close_code::PROTOCOL_ERROR),
    }
}

fn deliver(conn: &WsConn, op: u8, data: Bytes) -> FrameAction {
    if op == opcode::TEXT {
        // Validate UTF-8 in place and hand the Bytes view up — no re-parse
        // into a fresh String.
        if std::str::from_utf8(&data).is_err() {
            return FrameAction::InvalidUtf8;
        }
        return FrameAction::Text(conn.client_id.clone(), data);
    }
    FrameAction::Binary(conn.client_id.clone(), data)
}

// Server-to-client frames are never masked (RFC 6455 §5.1). A `Bytes` frame
// buffers once and is then shared (refcount bump) across broadcast queues.
fn encode(op: u8, payload: &[u8]) -> Bytes {
    let mut buf = BytesMut::with_capacity(8 + payload.len());
    encode_frame(true, op, payload, None, &mut buf);
    buf.freeze()
}

fn frame_text(data: &[u8]) -> Bytes {
    encode(opcode::TEXT, data)
}
fn frame_binary(data: &[u8]) -> Bytes {
    encode(opcode::BINARY, data)
}
fn frame_ping(data: &[u8]) -> Bytes {
    encode(opcode::PING, data)
}
fn frame_pong(data: &[u8]) -> Bytes {
    encode(opcode::PONG, data)
}
fn frame_close(code: u16, reason: &str) -> Bytes {
    encode(opcode::CLOSE, &close_payload(code, reason))
}
