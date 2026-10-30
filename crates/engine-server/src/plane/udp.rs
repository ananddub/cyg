//! Raw UDP plane: UDP wire format and seat auth.
//!
//! Runs ON THE SIM THREAD: `sim::run` registers the socket under `UDP_TOKEN`
//! and calls `on_readable` when readable, `sweep` once per loop pass.
//!
//! BIND handshake design: JOIN tokens are verified against `SeatRegistry`,
//! then a `SimCommand::BindPeer` (with a bounded(1) reply channel) is queued
//! and the peer is marked `pending_bind`. The sim drains commands after this
//! handler returns; on the next `on_readable`/`sweep` pass the reply is
//! drained, `player_id` is filled, `bound` flips true, and the BIND frame is
//! sent. Frames never block the poll loop.
//!
//! Two lanes share the socket, distinguished by the frame's first byte:
//!
//! * the plane's own `0x2x` control opcodes (`JOIN` … `DENY`), and
//! * the channel layer's `0x0x` frame opcodes, so a browser that already speaks
//!   `engine-webrtc-protocol` over a DataChannel speaks the same bytes here.
//!
//! `FrameKind::Reliable` outbound frames are NOT sent once and forgotten: each
//! bound peer owns a `ReliableLane` that retransmits until the peer answers with
//! `0x06` (`OP_ACK`). See [`crate::plane::reliable`] for the window, the RTT
//! estimate and why the resend is whole-window.

use std::cell::RefCell;
use std::collections::HashMap;
use std::io;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use crossbeam_channel::{Receiver, TryRecvError, bounded};
use mio::net::UdpSocket;
use mio::{Interest, Token};

use crate::plane::mono_ns;
use crate::plane::reliable::{ReliableCounts, ReliableLane};
use crate::plane::seats::SeatRegistry;
use engine_bus::{Bus, FrameKind, InputMsg, OutFrame, PeerId, SimCommand, Via};
use engine_webrtc_protocol::{EventFrame, OP_ACK, OP_EVENT, parse_ack};

/// mio registration token for the UDP socket inside the sim poll loop.
pub const UDP_TOKEN: Token = Token(1);

/// Per-peer inbound packet budget before drops kick in.
pub const MAX_PACKETS_PER_SEC: u32 = 512;

const OP_JOIN: u8 = 0x20;
const OP_INP: u8 = 0x21;
const OP_PING: u8 = 0x22;
const OP_PONG: u8 = 0x23;
const OP_BIND: u8 = 0x24;
const OP_DENY: u8 = 0x25;

const DENY_BAD_TOKEN: u8 = 1;
const DENY_FULL: u8 = 4;

/// [0x21][seq u16][payload 20]
const INP_LEN: usize = 23;
const SEND_RETRIES: u8 = 3;

struct TokenBucket {
    tokens: f64,
    last: Instant,
}

impl TokenBucket {
    fn new_at(now: Instant) -> TokenBucket {
        TokenBucket {
            tokens: MAX_PACKETS_PER_SEC as f64,
            last: now,
        }
    }

    fn allow(&mut self, now: Instant) -> bool {
        let elapsed = now.duration_since(self.last).as_secs_f64();
        self.last = now;
        self.tokens =
            (self.tokens + elapsed * MAX_PACKETS_PER_SEC as f64).min(MAX_PACKETS_PER_SEC as f64);
        if self.tokens >= 1.0 {
            self.tokens -= 1.0;
            true
        } else {
            false
        }
    }
}

struct Peer {
    session_id: PeerId,
    player_id: i64,
    bound: bool,
    last_seen: Instant,
    bucket: TokenBucket,
    /// `Some` while waiting for the sim to hand back the player id.
    pending_reply: Option<Receiver<i64>>,
    room: String,
    /// This peer's unacked events. Lives exactly as long as the peer entry: an
    /// idle-timeout drop discards the window with it, because a client that has
    /// gone silent for ten seconds is not going to ack any of them.
    reliable: ReliableLane,
}

pub struct UdpPlane {
    socket: RefCell<UdpSocket>,
    bus: Arc<Bus>,
    seats: Arc<SeatRegistry>,
    peers: HashMap<SocketAddr, Peer>,
    /// Reverse map: bus peer id (session string) → wire address.
    peer_addrs: HashMap<PeerId, SocketAddr>,
    /// Rate-limit state for addresses with no peer entry (JOIN spam).
    strangers: HashMap<SocketAddr, TokenBucket>,
    rx_buf: Vec<u8>,
    scratch: Vec<u8>,
    session_counter: u64,
    tick_hz: f64,
    /// How long a peer may go silent before the plane drops it. The host loop
    /// honours `[lifecycle] peer_idle_ms`; this lane has to agree or the two
    /// transports would disconnect at different times.
    peer_idle: Duration,
    pkts_in: u64,
    denied: u64,
    rate_limited: u64,
    sends: u64,
    send_drops: u64,
    recv_errors: u64,
    /// Reliable frames the lane never saw: unknown or unbound peer, a frame that
    /// is not one event, or a full window. Each is a producer-side problem, not
    /// a network loss, so it is counted separately from `send_drops`.
    reliable_refused: u64,
    /// Inbound acks from addresses with no bound peer (spoofed or stale).
    acks_ignored: u64,
    /// Inbound `0x03` event frames. The plane's fire path is the input payload's
    /// fire flag, so these are recognised for telemetry and not applied.
    events_in: u64,
    /// Events a peer has actually acknowledged.
    events_acked: u64,
}

impl UdpPlane {
    pub fn new(
        bind_addr: &str,
        bus: Arc<Bus>,
        seats: Arc<SeatRegistry>,
        peer_idle: Duration,
    ) -> io::Result<UdpPlane> {
        let addr: SocketAddr = bind_addr
            .parse()
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?;
        let std_socket = bind_udp(addr)?;
        let socket = RefCell::new(UdpSocket::from_std(std_socket));
        // PlaneConfig::tick_hz is not passed through `new`'s signature; read
        // the same env knob (default 128) and allow override via `set_tick_hz`.
        let tick_hz = std::env::var("OX_TICK_HZ")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(128.0);
        Ok(UdpPlane {
            socket,
            bus,
            seats,
            peers: HashMap::new(),
            peer_addrs: HashMap::new(),
            strangers: HashMap::new(),
            rx_buf: vec![0u8; 65536],
            scratch: Vec::with_capacity(128),
            session_counter: 0,
            tick_hz,
            peer_idle,
            pkts_in: 0,
            denied: 0,
            rate_limited: 0,
            sends: 0,
            send_drops: 0,
            recv_errors: 0,
            reliable_refused: 0,
            acks_ignored: 0,
            events_in: 0,
            events_acked: 0,
        })
    }

    /// The bound address. Meaningful with port `0` ("let the kernel pick"), which
    /// is how the plane is tested without colliding with a live `:7778`.
    pub fn local_addr(&self) -> io::Result<SocketAddr> {
        self.socket.borrow().local_addr()
    }

    /// Override the tick rate advertised in the BIND frame (call once at boot
    /// from `PlaneConfig.tick_hz` if it differs from the env default).
    pub fn set_tick_hz(&mut self, hz: f64) {
        self.tick_hz = hz;
    }

    /// Register the socket with the sim thread's poll (interest: READABLE).
    pub fn register(&self, poll: &mio::Poll) -> io::Result<()> {
        let mut sock = self.socket.borrow_mut();
        poll.registry()
            .register(&mut *sock, UDP_TOKEN, Interest::READABLE)
    }

    /// Drain the kernel receive queue. Handles JOIN/BIND auth, INP →
    /// `bus.push_input` with `arrival_ns = mono_ns()` stamped at read time,
    /// rate limiting, and DENY replies.
    pub fn on_readable(&mut self) {
        self.drain_pending();
        let mut sock = self.socket.borrow_mut();
        loop {
            let (len, addr) = match sock.recv_from(&mut self.rx_buf) {
                Ok(r) => r,
                Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => break,
                Err(_) => {
                    self.recv_errors += 1;
                    break;
                }
            };
            // Kernel→sim latency sample: stamp immediately after the syscall.
            let arrival_ns = mono_ns();
            self.pkts_in += 1;
            if len == 0 {
                continue;
            }
            let now = Instant::now();
            let allowed = match self.peers.get_mut(&addr) {
                Some(p) => p.bucket.allow(now),
                None => self
                    .strangers
                    .entry(addr)
                    .or_insert_with(|| TokenBucket::new_at(now))
                    .allow(now),
            };
            if !allowed {
                self.rate_limited += 1;
                continue;
            }
            if let Some(p) = self.peers.get_mut(&addr) {
                p.last_seen = now;
            }

            let b = &self.rx_buf[..len];
            match b[0] {
                OP_JOIN => {
                    if len < 2 || b.len() != 2 + b[1] as usize {
                        continue;
                    }
                    let Ok(token) = std::str::from_utf8(&b[2..]) else {
                        Self::send_deny(
                            &mut sock,
                            &mut self.scratch,
                            &mut self.denied,
                            addr,
                            DENY_BAD_TOKEN,
                        );
                        continue;
                    };
                    let grant = self.seats.verify(token);
                    match grant {
                        None => Self::send_deny(
                            &mut sock,
                            &mut self.scratch,
                            &mut self.denied,
                            addr,
                            DENY_BAD_TOKEN,
                        ),
                        Some(grant) => {
                            if self.peers.contains_key(&addr) {
                                continue; // already pending or bound
                            }
                            let (tx, rx) = bounded::<i64>(1);
                            let session_id: PeerId =
                                format!("udp-{}-{}", addr, self.session_counter);
                            self.session_counter += 1;
                            let cmd = SimCommand::BindPeer {
                                peer_id: session_id.clone(),
                                room: grant.room.clone(),
                                reply: tx,
                            };
                            if self.bus.commands_tx.try_send(cmd).is_ok() {
                                self.strangers.remove(&addr);
                                self.peer_addrs.insert(session_id.clone(), addr);
                                self.peers.insert(
                                    addr,
                                    Peer {
                                        session_id,
                                        player_id: 0,
                                        bound: false,
                                        last_seen: now,
                                        bucket: TokenBucket::new_at(now),
                                        pending_reply: Some(rx),
                                        room: grant.room,
                                        reliable: ReliableLane::new(),
                                    },
                                );
                            } else {
                                Self::send_deny(
                                    &mut sock,
                                    &mut self.scratch,
                                    &mut self.denied,
                                    addr,
                                    DENY_FULL,
                                );
                            }
                        }
                    }
                }
                OP_INP => {
                    if len != INP_LEN {
                        continue;
                    }
                    // Unknown or not-yet-bound addresses are dropped silently.
                    let pid = match self.peers.get(&addr) {
                        Some(p) if p.bound => p.player_id,
                        _ => continue,
                    };
                    let mut payload = [0u8; 20];
                    payload.copy_from_slice(&b[3..23]);
                    let seq = u16::from_le_bytes([b[1], b[2]]);
                    self.bus.push_input(InputMsg {
                        player_id: pid,
                        seq,
                        payload,
                        arrival_ns,
                        via: Via::NativeUdp,
                    });
                }
                OP_PING => {
                    if len < 9 {
                        continue;
                    }
                    self.scratch.clear();
                    self.scratch.push(OP_PONG);
                    self.scratch.extend_from_slice(&b[1..9]);
                    if sock.send_to(&self.scratch, addr).is_ok() {
                        self.sends += 1;
                    } else {
                        self.send_drops += 1;
                    }
                }
                // Channel-layer ack: `[0x06][seq u16][kind u8]`, cumulative for
                // this peer's event window. Only a bound peer owns a window, so
                // a spoofed ack from any other address releases nothing.
                OP_ACK => {
                    // `parse_ack` checks the exact 4-byte length itself, so a
                    // longer guest payload that starts with 0x06 is not an ack.
                    let Some(ack) = parse_ack(b) else { continue };
                    let Some(peer) = self.peers.get_mut(&addr) else {
                        self.acks_ignored += 1;
                        continue;
                    };
                    if !peer.bound {
                        self.acks_ignored += 1;
                        continue;
                    }
                    let released = peer.reliable.ack(ack.seq, mono_ns() / 1_000_000) as u64;
                    self.events_acked += released;
                }
                OP_EVENT => {
                    self.events_in += 1;
                }
                _ => {} // other opcodes (incl. delta chunks) dropped
            }
        }
    }

    /// Queue or send one outbound frame to a bound peer.
    ///
    /// A `FrameKind::Unreliable` frame is exactly one datagram, newest-wins, and
    /// false means the peer is unknown or the socket refused it. A
    /// `FrameKind::Reliable` frame goes to that peer's [`ReliableLane`] instead:
    /// true means the lane **accepted** the event and will keep resending it
    /// until the peer acks — not that a datagram left the socket on this call.
    pub fn send_frame(&mut self, frame: &OutFrame) -> bool {
        if frame.kind == FrameKind::Reliable {
            return self.queue_reliable(&frame.peer_id, &frame.bytes);
        }
        let Some(&addr) = self.peer_addrs.get(frame.peer_id.as_str()) else {
            return false;
        };
        let mut sock = self.socket.borrow_mut();
        match Self::send_raw(&mut sock, &frame.bytes, addr) {
            Ok(()) => {
                self.sends += 1;
                true
            }
            Err(_) => {
                self.send_drops += 1;
                false
            }
        }
    }

    /// Hand one framed event to its peer's window and transmit it immediately,
    /// so an event never waits for the next tick to leave the socket.
    fn queue_reliable(&mut self, peer_id: &str, bytes: &[u8]) -> bool {
        // The seq the lane tracks must be the seq the client reads out of the
        // frame, so an ack names the same number. A reliable frame that is not
        // one event is a producer bug: refuse it rather than resend blindly.
        let Ok(event) = EventFrame::decode(bytes) else {
            self.reliable_refused += 1;
            return false;
        };
        let Some(&addr) = self.peer_addrs.get(peer_id) else {
            self.reliable_refused += 1;
            return false;
        };
        let now = mono_ns() / 1_000_000;
        let refused = {
            let Some(peer) = self.peers.get_mut(&addr) else {
                self.reliable_refused += 1;
                return false;
            };
            if !peer.bound {
                self.reliable_refused += 1;
                return false;
            }
            // The lane owns its resend buffer, so the frame is copied out of the
            // caller's `OutFrame`. Events are fixed-size and rare; deltas, which
            // are neither, never take this path.
            peer.reliable.queue(event.seq, bytes.to_vec(), now).is_err()
        };
        if refused {
            self.reliable_refused += 1;
            return false;
        }
        self.pump_peer(addr, now);
        true
    }

    /// Run one peer's retransmit timers. The socket is borrowed per datagram
    /// inside the pump, never across the lane, so a full window of resends can
    /// not hold the socket away from anything else in the loop pass.
    fn pump_peer(&mut self, addr: SocketAddr, now_ms: u64) {
        let socket = &self.socket;
        let peers = &mut self.peers;
        let Some(peer) = peers.get_mut(&addr) else {
            return;
        };
        let mut sent = 0u64;
        let mut drops = 0u64;
        peer.reliable.pump(now_ms, |wire| {
            let mut sock = socket.borrow_mut();
            match Self::send_raw(&mut sock, wire, addr) {
                Ok(()) => {
                    sent += 1;
                    true
                }
                Err(_) => {
                    drops += 1;
                    false
                }
            }
        });
        self.sends += sent;
        self.send_drops += drops;
    }

    /// Retransmit every peer's overdue events and retire the dead ones.
    /// `sim::run` calls this once per loop pass, so the poll deadline (≤ one
    /// tick, ~8 ms at 128 Hz) bounds how late a resend can run behind its timer.
    pub fn pump_reliable(&mut self) {
        let now = mono_ns() / 1_000_000;
        let due: Vec<SocketAddr> = self
            .peers
            .iter()
            .filter(|(_, p)| p.bound && p.reliable.inflight() > 0)
            .map(|(addr, _)| *addr)
            .collect();
        for addr in due {
            self.pump_peer(addr, now);
        }
    }

    /// Aggregate reliable counters over every peer currently in the table.
    pub fn reliable_counts(&self) -> ReliableCounts {
        let mut total = ReliableCounts::default();
        for peer in self.peers.values() {
            let c = peer.reliable.counts();
            total.queued += c.queued;
            total.sent += c.sent;
            total.resent += c.resent;
            total.acked += c.acked;
            total.expired += c.expired;
            total.rejected += c.rejected;
        }
        total
    }

    /// Events still waiting for an ack.
    pub fn reliable_inflight(&self) -> usize {
        self.peers.values().map(|p| p.reliable.inflight()).sum()
    }

    /// Reliable frames the plane refused to queue (see [`UdpPlane::reliable_refused`]).
    pub fn reliable_refused(&self) -> u64 {
        self.reliable_refused
    }

    /// Inbound acks from an address that owns no bound peer.
    pub fn acks_ignored(&self) -> u64 {
        self.acks_ignored
    }

    /// Events acknowledged by a peer.
    pub fn events_acked(&self) -> u64 {
        self.events_acked
    }

    /// Inbound event frames (recognised, not applied; see `events_in`).
    pub fn events_in(&self) -> u64 {
        self.events_in
    }

    /// Smoothed round trip of one peer's lane in ms, or `None` if the peer is
    /// unknown, unbound, or has never acked anything.
    pub fn peer_rtt_ms(&self, peer_id: &str) -> Option<f64> {
        let &addr = self.peer_addrs.get(peer_id)?;
        let peer = self.peers.get(&addr)?;
        peer.bound
            .then_some(peer.reliable.rtt_ms())
            .filter(|ms| *ms > 0.0)
    }

    /// Idle-timeout sweep (`peer_idle` without a packet): unbinds the peer and
    /// queues `SimCommand::DropPeer` on the bus. Also drains pending BIND
    /// replies and prunes stranger rate-limit buckets.
    pub fn sweep(&mut self) {
        self.drain_pending();
        let now = Instant::now();
        let peer_idle = self.peer_idle;
        let mut dropped: Vec<PeerId> = Vec::new();
        self.peers.retain(|_, peer| {
            if now.duration_since(peer.last_seen) > peer_idle {
                dropped.push(peer.session_id.clone());
                false
            } else {
                true
            }
        });
        for sid in dropped {
            self.peer_addrs.remove(&sid);
            let _ = self
                .bus
                .commands_tx
                .try_send(SimCommand::DropPeer { peer_id: sid });
        }
        self.strangers
            .retain(|_, bucket| now.duration_since(bucket.last) <= peer_idle);
    }

    /// Number of currently bound peers (for status).
    pub fn peer_count(&self) -> usize {
        self.peers
            .values()
            .filter(|p| p.bound || p.pending_reply.is_some())
            .count()
    }

    /// (pkts_in, denied, rate_limited, sends, send_drops)
    pub fn stats(&self) -> (u64, u64, u64, u64, u64) {
        (
            self.pkts_in,
            self.denied,
            self.rate_limited,
            self.sends,
            self.send_drops,
        )
    }

    /// Fill `player_id` from sim replies and emit BIND frames. Safe to call
    /// every pass: `try_recv` never blocks.
    pub fn drain_pending(&mut self) {
        let mut rejected: Vec<SocketAddr> = Vec::new();
        for (&addr, peer) in self.peers.iter_mut() {
            let got = match peer.pending_reply.as_mut() {
                Some(rx) => match rx.try_recv() {
                    Ok(id) => Some(id),
                    Err(TryRecvError::Disconnected) => None,
                    Err(TryRecvError::Empty) => None,
                },
                None => None,
            };
            if let Some(id) = got {
                peer.pending_reply = None;
                if id < 0 {
                    // Sim declined (room full / unknown): DENY(4), not a
                    // BIND carrying a fake player id.
                    let mut sock = self.socket.borrow_mut();
                    Self::send_deny(&mut sock, &mut self.scratch, &mut self.denied, addr, 4);
                    drop(sock);
                    rejected.push(addr);
                    continue;
                }
                peer.player_id = id;
                peer.bound = true;
                self.scratch.clear();
                self.scratch.push(OP_BIND);
                self.scratch.extend_from_slice(&id.to_le_bytes());
                self.scratch.extend_from_slice(&self.tick_hz.to_le_bytes());
                let room = &peer.room;
                self.scratch.push(room.len().min(255) as u8);
                self.scratch.extend_from_slice(room.as_bytes());
                let mut sock = self.socket.borrow_mut();
                match Self::send_raw(&mut sock, &self.scratch, addr) {
                    Ok(()) => self.sends += 1,
                    Err(_) => self.send_drops += 1,
                }
            }
        }
        for addr in rejected {
            if let Some(peer) = self.peers.remove(&addr) {
                self.peer_addrs.remove(&peer.session_id);
            }
        }
    }

    fn send_deny(
        sock: &mut UdpSocket,
        scratch: &mut Vec<u8>,
        denied: &mut u64,
        addr: SocketAddr,
        reason: u8,
    ) {
        scratch.clear();
        scratch.push(OP_DENY);
        scratch.push(reason);
        if sock.send_to(scratch, addr).is_ok() {
            *denied += 1;
        }
    }

    fn send_raw(sock: &mut UdpSocket, bytes: &[u8], addr: SocketAddr) -> io::Result<()> {
        let mut tries: u8 = 0;
        loop {
            match sock.send_to(bytes, addr) {
                Ok(_) => return Ok(()),
                Err(ref e) if e.kind() == io::ErrorKind::WouldBlock && tries < SEND_RETRIES => {
                    tries += 1;
                }
                Err(e) => return Err(e),
            }
        }
    }
}

/// socket2 is not a dependency of this crate; replicate its bind pattern
/// (nonblocking + SO_REUSEADDR/SO_REUSEPORT) with libc on unix.
#[cfg(unix)]
fn bind_udp(addr: SocketAddr) -> io::Result<std::net::UdpSocket> {
    use std::os::unix::io::FromRawFd;
    let domain = if addr.is_ipv6() {
        libc::AF_INET6
    } else {
        libc::AF_INET
    };
    let fd = unsafe {
        libc::socket(
            domain,
            libc::SOCK_DGRAM | libc::SOCK_NONBLOCK,
            libc::IPPROTO_UDP,
        )
    };
    if fd < 0 {
        return Err(io::Error::last_os_error());
    }
    let sock = unsafe { std::net::UdpSocket::from_raw_fd(fd) };
    let on: libc::c_int = 1;
    let len = std::mem::size_of::<libc::c_int>() as libc::socklen_t;
    unsafe {
        libc::setsockopt(
            fd,
            libc::SOL_SOCKET,
            libc::SO_REUSEADDR,
            (&on as *const libc::c_int).cast(),
            len,
        );
        #[cfg(any(
            target_os = "linux",
            target_os = "android",
            target_os = "macos",
            target_os = "ios",
            target_os = "freebsd",
            target_os = "dragonfly",
            target_os = "netbsd",
            target_os = "openbsd"
        ))]
        libc::setsockopt(
            fd,
            libc::SOL_SOCKET,
            libc::SO_REUSEPORT,
            (&on as *const libc::c_int).cast(),
            len,
        );
    }
    let rc = match addr {
        SocketAddr::V4(a) => {
            let mut sa: libc::sockaddr_in = unsafe { std::mem::zeroed() };
            sa.sin_family = libc::AF_INET as libc::sa_family_t;
            sa.sin_port = a.port().to_be();
            sa.sin_addr = libc::in_addr {
                s_addr: u32::from(a.ip().to_bits()).to_be(),
            };
            unsafe {
                libc::bind(
                    fd,
                    (&sa as *const libc::sockaddr_in).cast(),
                    std::mem::size_of_val(&sa) as libc::socklen_t,
                )
            }
        }
        SocketAddr::V6(a) => {
            let mut sa: libc::sockaddr_in6 = unsafe { std::mem::zeroed() };
            sa.sin6_family = libc::AF_INET6 as libc::sa_family_t;
            sa.sin6_port = a.port().to_be();
            sa.sin6_addr = libc::in6_addr {
                s6_addr: a.ip().octets(),
            };
            sa.sin6_scope_id = a.scope_id();
            unsafe {
                libc::bind(
                    fd,
                    (&sa as *const libc::sockaddr_in6).cast(),
                    std::mem::size_of_val(&sa) as libc::socklen_t,
                )
            }
        }
    };
    if rc < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(sock)
}

#[cfg(not(unix))]
fn bind_udp(addr: SocketAddr) -> io::Result<std::net::UdpSocket> {
    let sock = std::net::UdpSocket::bind(addr)?;
    sock.set_nonblocking(true)?;
    Ok(sock)
}
