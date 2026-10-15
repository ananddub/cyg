//! Raw UDP plane (Agent 3B). See CONTRACT.md §UDP wire.
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

use std::cell::RefCell;
use std::collections::HashMap;
use std::io;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use crossbeam_channel::{bounded, Receiver, TryRecvError};
use mio::net::UdpSocket;
use mio::{Interest, Token};

use engine_bus::{Bus, InputMsg, OutFrame, PeerId, SimCommand, Via};
use crate::plane::mono_ns;
use crate::plane::seats::SeatRegistry;

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
const IDLE_TIMEOUT: Duration = Duration::from_secs(10);
const SEND_RETRIES: u8 = 3;

struct TokenBucket {
    tokens: f64,
    last: Instant,
}

impl TokenBucket {
    fn new_at(now: Instant) -> TokenBucket {
        TokenBucket { tokens: MAX_PACKETS_PER_SEC as f64, last: now }
    }

    fn allow(&mut self, now: Instant) -> bool {
        let elapsed = now.duration_since(self.last).as_secs_f64();
        self.last = now;
        self.tokens = (self.tokens + elapsed * MAX_PACKETS_PER_SEC as f64)
            .min(MAX_PACKETS_PER_SEC as f64);
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
    pkts_in: u64,
    denied: u64,
    rate_limited: u64,
    sends: u64,
    send_drops: u64,
    recv_errors: u64,
}

impl UdpPlane {
    pub fn new(bind_addr: &str, bus: Arc<Bus>, seats: Arc<SeatRegistry>) -> io::Result<UdpPlane> {
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
            pkts_in: 0,
            denied: 0,
            rate_limited: 0,
            sends: 0,
            send_drops: 0,
            recv_errors: 0,
        })
    }

    /// Override the tick rate advertised in the BIND frame (call once at boot
    /// from `PlaneConfig.tick_hz` if it differs from the env default).
    pub fn set_tick_hz(&mut self, hz: f64) {
        self.tick_hz = hz;
    }

    /// Register the socket with the sim thread's poll (interest: READABLE).
    pub fn register(&self, poll: &mio::Poll) -> io::Result<()> {
        let mut sock = self.socket.borrow_mut();
        poll.registry().register(&mut *sock, UDP_TOKEN, Interest::READABLE)
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
                        Self::send_deny(&mut sock, &mut self.scratch, &mut self.denied, addr, DENY_BAD_TOKEN);
                        continue;
                    };
                    let grant = self.seats.verify(token);
                    match grant {
                        None => Self::send_deny(&mut sock, &mut self.scratch, &mut self.denied, addr, DENY_BAD_TOKEN),
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
                                    },
                                );
                            } else {
                                Self::send_deny(&mut sock, &mut self.scratch, &mut self.denied, addr, DENY_FULL);
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
                _ => {} // other opcodes (incl. channel frames) dropped
            }
        }
    }

    /// One outbound framed message to a bound peer. False = unknown peer.
    pub fn send_frame(&mut self, frame: &OutFrame) -> bool {
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

    /// Idle-timeout sweep (~10 s without a packet): unbinds the peer and
    /// queues `SimCommand::DropPeer` on the bus. Also drains pending BIND
    /// replies and prunes stranger rate-limit buckets.
    pub fn sweep(&mut self) {
        self.drain_pending();
        let now = Instant::now();
        let mut dropped: Vec<PeerId> = Vec::new();
        self.peers.retain(|_, peer| {
            if now.duration_since(peer.last_seen) > IDLE_TIMEOUT {
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
            .retain(|_, bucket| now.duration_since(bucket.last) <= IDLE_TIMEOUT);
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
    let domain = if addr.is_ipv6() { libc::AF_INET6 } else { libc::AF_INET };
    let fd = unsafe { libc::socket(domain, libc::SOCK_DGRAM | libc::SOCK_NONBLOCK, libc::IPPROTO_UDP) };
    if fd < 0 {
        return Err(io::Error::last_os_error());
    }
    let sock = unsafe { std::net::UdpSocket::from_raw_fd(fd) };
    let on: libc::c_int = 1;
    let len = std::mem::size_of::<libc::c_int>() as libc::socklen_t;
    unsafe {
        libc::setsockopt(fd, libc::SOL_SOCKET, libc::SO_REUSEADDR, (&on as *const libc::c_int).cast(), len);
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
        libc::setsockopt(fd, libc::SOL_SOCKET, libc::SO_REUSEPORT, (&on as *const libc::c_int).cast(), len);
    }
    let rc = match addr {
        SocketAddr::V4(a) => {
            let mut sa: libc::sockaddr_in = unsafe { std::mem::zeroed() };
            sa.sin_family = libc::AF_INET as libc::sa_family_t;
            sa.sin_port = a.port().to_be();
            sa.sin_addr = libc::in_addr { s_addr: u32::from(a.ip().to_bits()).to_be() };
            unsafe {
                libc::bind(fd, (&sa as *const libc::sockaddr_in).cast(), std::mem::size_of_val(&sa) as libc::socklen_t)
            }
        }
        SocketAddr::V6(a) => {
            let mut sa: libc::sockaddr_in6 = unsafe { std::mem::zeroed() };
            sa.sin6_family = libc::AF_INET6 as libc::sa_family_t;
            sa.sin6_port = a.port().to_be();
            sa.sin6_addr = libc::in6_addr { s6_addr: a.ip().octets() };
            sa.sin6_scope_id = a.scope_id();
            unsafe {
                libc::bind(fd, (&sa as *const libc::sockaddr_in6).cast(), std::mem::size_of_val(&sa) as libc::socklen_t)
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
