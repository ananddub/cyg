//! # Raw mio WebSocket Server
//!
//! Thread-per-core friendly, no async runtime — the same philosophy as
//! engine-net's `GameTransport`: `poll_events(timeout)` drains the epoll
//! batch, handlers get owned events, writes are queued per connection and
//! flushed on `WRITABLE`.
//!
//! State machine per socket: `Handshaking -> Open -> CloseSent -> (removed)`.
//! Ping is auto-answered with Pong; Close echoes the close frame once.

use std::collections::{HashMap, VecDeque};
use std::io::{self, Read, Write};
use std::net::{SocketAddr, TcpListener as StdTcpListener};
use std::time::Duration;

use mio::net::TcpStream;
use mio::{Events, Interest, Poll, Token, Waker};
use socket2::{Domain, Protocol, Socket, Type};

use crate::frame::{
    Frame, FrameDecoder, FrameError, close_code, close_payload, encode_frame, opcode,
    parse_close,
};
use crate::handshake::{HandshakeOutcome, HandshakeReader, accept_response, reject_response};

const LISTENER: Token = Token(0);
const WAKER: Token = Token(1);

/// Events surfaced to the game/engine side of the wire.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WsEvent {
    Connected {
        client_id: String,
        peer_addr: SocketAddr,
        path: String,
    },
    Disconnected {
        client_id: String,
        code: u16,
    },
    Text {
        client_id: String,
        text: String,
    },
    Binary {
        client_id: String,
        data: Vec<u8>,
    },
    /// A Pong reply to a server-initiated Ping (RTT probe).
    Pong {
        client_id: String,
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
    Frames(Vec<Frame>),
    ProtocolViolation(u16),
}

struct WsConn {
    socket: TcpStream,
    peer_addr: SocketAddr,
    client_id: String,
    state: ConnState,
    handshake: HandshakeReader,
    path: String,
    reader: FrameDecoder,
    frag_buf: Vec<u8>,
    frag_opcode: u8,
    write_queue: VecDeque<u8>,
    write_wanted: bool,
    /// After flushing a rejection/101 we may need to drop the socket.
    drop_after_flush: bool,
}

impl WsConn {
    fn queue(&mut self, bytes: &[u8]) {
        self.write_queue.extend(bytes.iter().copied());
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
        })
    }

    pub fn local_addr(&self) -> io::Result<SocketAddr> {
        self.listener.local_addr()
    }

    /// Open (post-handshake) connections.
    pub fn ccu(&self) -> usize {
        self.conns.values().filter(|c| c.state == ConnState::Open).count()
    }

    /// Drive the event loop once; returns all engine-visible events.
    pub fn poll_events(&mut self, timeout: Duration) -> io::Result<Vec<WsEvent>> {
        self.poll.poll(&mut self.events, Some(timeout))?;

        let mut readable = Vec::new();
        let mut writable = Vec::new();
        let mut have_listener = false;
        for ev in self.events.iter() {
            match ev.token() {
                LISTENER => have_listener = true,
                WAKER => {},
                t => {
                    if ev.is_readable() {
                        readable.push(t);
                    }
                    if ev.is_writable() {
                        writable.push(t);
                    }
                },
            }
        }

        let mut out = Vec::new();
        if have_listener {
            self.accept_all(&mut out);
        }
        for token in writable {
            self.flush(token);
        }
        for token in readable {
            self.on_readable(token, &mut out);
        }
        Ok(out)
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

    fn broadcast(&mut self, frame: Vec<u8>) {
        let ids: Vec<String> = self
            .conns
            .values()
            .filter(|c| c.state == ConnState::Open)
            .map(|c| c.client_id.clone())
            .collect();
        for id in ids {
            let _ = self.send_frame(&id, frame.clone());
        }
    }

    fn send_frame(&mut self, client_id: &str, frame: Vec<u8>) -> io::Result<()> {
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
            let mut out = Vec::new();
            self.drop_conn(token, close_code::POLICY_VIOLATION, &mut out);
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
                    let client_id = format!("ws-{}", self.next_client_id);
                    self.next_client_id += 1;
                    let conn = WsConn {
                        socket,
                        peer_addr,
                        client_id: client_id.clone(),
                        state: ConnState::Handshaking,
                        handshake: HandshakeReader::new(),
                        path: String::new(),
                        reader: FrameDecoder::new(self.config.max_frame_bytes),
                        frag_buf: Vec::new(),
                        frag_opcode: 0,
                        write_queue: VecDeque::new(),
                        write_wanted: false,
                        drop_after_flush: false,
                    };
                    self.by_client.insert(client_id, token.0);
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
            let mut buf = [0u8; 16 * 1024];
            let step = {
                let Some(conn) = self.conns.get_mut(&token.0) else {
                    return;
                };
                match conn.socket.read(&mut buf) {
                    Ok(0) => Step::ProtocolViolation(close_code::GOING_AWAY),
                    Ok(n) => match conn.state {
                        ConnState::Handshaking => {
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
                        ConnState::Open | ConnState::CloseSent => {
                            match conn.reader.feed(&buf[..n]) {
                                Ok(frames) => Step::Frames(frames),
                                Err(FrameError::Protocol(_)) => {
                                    Step::ProtocolViolation(close_code::PROTOCOL_ERROR)
                                },
                                Err(FrameError::TooBig(_)) => {
                                    Step::ProtocolViolation(close_code::TOO_BIG)
                                },
                            }
                        },
                    },
                    Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => return,
                    Err(_) => Step::ProtocolViolation(close_code::GOING_AWAY),
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
                Step::Frames(frames) => {
                    if !self.handle_frames(token, frames, out) {
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

    /// Returns `false` when the connection was removed mid-processing.
    fn handle_frames(&mut self, token: Token, frames: Vec<Frame>, out: &mut Vec<WsEvent>) -> bool {
        for frame in frames {
            let action = {
                let Some(conn) = self.conns.get_mut(&token.0) else {
                    return false;
                };
                classify_frame(conn, frame)
            };
            match action {
                FrameAction::None => {},
                FrameAction::Text(id, text) => out.push(WsEvent::Text { client_id: id, text }),
                FrameAction::Binary(id, data) => out.push(WsEvent::Binary { client_id: id, data }),
                FrameAction::Pong(id) => out.push(WsEvent::Pong { client_id: id }),
                FrameAction::SendPong(data) => {
                    if let Some(conn) = self.conns.get_mut(&token.0) {
                        conn.queue(&frame_pong(&data));
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
                    if let Some(conn) = self.conns.get_mut(&token.0) {
                        conn.queue(&frame_close(close_code::INVALID_PAYLOAD, ""));
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
            self.by_client.remove(&conn.client_id);
            if conn.state != ConnState::Handshaking {
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
        let mut chunk = Vec::new();
        loop {
            chunk.clear();
            let take = conn.write_queue.len().min(64 * 1024);
            chunk.extend(conn.write_queue.iter().copied().take(take));
            if chunk.is_empty() {
                break;
            }
            match conn.socket.write(&chunk) {
                Ok(0) => break,
                Ok(n) => {
                    conn.write_queue.drain(..n);
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
            let mut out = Vec::new();
            self.drop_conn(token, close_code::NORMAL, &mut out);
        }
    }
}

fn not_connected() -> io::Error {
    io::Error::new(io::ErrorKind::NotConnected, "ws client not connected")
}

enum FrameAction {
    None,
    Text(String, String),
    Binary(String, Vec<u8>),
    Pong(String),
    SendPong(Vec<u8>),
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
                (conn.frag_opcode, std::mem::take(&mut conn.frag_buf))
            };
            deliver(conn, final_op, data)
        },
        opcode::CONTINUATION => {
            conn.frag_buf.extend_from_slice(&payload);
            if !fin {
                return FrameAction::None;
            }
            let op = conn.frag_opcode;
            let data = std::mem::take(&mut conn.frag_buf);
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

fn deliver(conn: &WsConn, op: u8, data: Vec<u8>) -> FrameAction {
    if op == opcode::TEXT {
        match String::from_utf8(data) {
            Ok(text) => FrameAction::Text(conn.client_id.clone(), text),
            Err(_) => FrameAction::InvalidUtf8,
        }
    } else {
        FrameAction::Binary(conn.client_id.clone(), data)
    }
}

// Server-to-client frames are never masked (RFC 6455 §5.1).
fn encode(op: u8, payload: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(8 + payload.len());
    encode_frame(true, op, payload, None, &mut out);
    out
}

fn frame_text(data: &[u8]) -> Vec<u8> {
    encode(opcode::TEXT, data)
}
fn frame_binary(data: &[u8]) -> Vec<u8> {
    encode(opcode::BINARY, data)
}
fn frame_ping(data: &[u8]) -> Vec<u8> {
    encode(opcode::PING, data)
}
fn frame_pong(data: &[u8]) -> Vec<u8> {
    encode(opcode::PONG, data)
}
fn frame_close(code: u16, reason: &str) -> Vec<u8> {
    encode(opcode::CLOSE, &close_payload(code, reason))
}
