//! # Wire transports
//!
//! One folder per transport, plus the [`GameTransport`] trait that
//! [`create_transport`] selects through [`TransportType`]:
//!
//! - `udp`: raw non-blocking length-prefixed datagrams
//! - `ws`: the CYG/Colyseus WebSocket wire protocol (the browser plane)
//! - `webrtc`: sans-IO `str0m` DataChannels and their channel contract
//!
//! Only `udp` and `ws` implement [`GameTransport`]; the WebRTC server is driven
//! directly by the game loop because what it moves is framed deltas, not packets.
//! Either way the simulation above this module does not know which wire a client
//! reached the room on.

pub mod quic_transport;
pub mod udp;
pub mod webrtc;
pub mod ws;

use std::io;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use bytes::Bytes;

use crate::framing::OpCode;

pub use quic_transport::QuicTransport;
pub use udp::UdpTransport;
pub use webrtc::{WebRtcServer, WebRtcServerEvent, WebRtcTransport};
pub use ws::WsTransport;

/// Supported transport implementations that sit behind [`GameTransport`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransportType {
    /// Raw high-frequency non-blocking UDP datagrams.
    Udp,
    /// WebSocket with CYG SDK wire-protocol compatibility (zero client migration).
    Ws,
}

impl std::str::FromStr for TransportType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "udp" => Ok(Self::Udp),
            "colyseus" | "colyseus-ws" | "ws" => Ok(Self::Ws),
            _ => Err(format!("Unknown transport type: {}", s)),
        }
    }
}

/// Incoming transport events dispatched to the game simulation.
///
/// `client_id` is an [`Arc<str>`]: per-event/per-packet clones are refcount
/// bumps instead of string allocations.
#[derive(Debug, Clone)]
pub enum TransportEvent {
    Connected {
        client_id: Arc<str>,
        peer_addr: SocketAddr,
    },
    Disconnected {
        client_id: Arc<str>,
    },
    Packet {
        client_id: Arc<str>,
        opcode: OpCode,
        payload: Bytes,
    },
}

/// Pluggable transport trait implemented by all network layers.
pub trait GameTransport: Send {
    /// Poll for network events with a bounded microsecond timeout, appending
    /// them to a caller-owned buffer. Reusing one `Vec` across polls keeps the
    /// hot loop allocation-free.
    fn poll_events(&mut self, timeout: Duration, events: &mut Vec<TransportEvent>) -> io::Result<()>;

    /// Send a message to a specific client session.
    fn send_to(&mut self, client_id: &str, opcode: OpCode, payload: &[u8]) -> io::Result<()>;

    /// Send the same message to several sessions. Transports may encode once
    /// for all recipients; the default preserves per-session behaviour.
    fn send_to_many(
        &mut self,
        client_ids: &[&str],
        opcode: OpCode,
        payload: &[u8],
    ) -> io::Result<()> {
        for client_id in client_ids {
            self.send_to(client_id, opcode, payload)?;
        }
        Ok(())
    }

    /// Broadcast a message to all connected clients.
    fn broadcast(&mut self, opcode: OpCode, payload: &[u8]) -> io::Result<()>;

    /// Politely disconnect a client (server-initiated leave). Default: no-op.
    fn disconnect(&mut self, _client_id: &str) -> io::Result<()> {
        Ok(())
    }

    /// Server-initiated close with an explicit RFC 6455 close code and reason,
    /// e.g. `close_code::WITH_ERROR` when a join is rejected. Default: falls
    /// back to [`GameTransport::disconnect`].
    fn disconnect_with(&mut self, client_id: &str, _code: u16, _reason: &str) -> io::Result<()> {
        self.disconnect(client_id)
    }

    /// Flush batched pending writes to operating system sockets.
    fn flush(&mut self) -> io::Result<()>;

    /// Current number of concurrent connected users (CCU).
    fn ccu(&self) -> usize;

    /// Bound socket address.
    fn local_addr(&self) -> io::Result<SocketAddr>;

    /// Transport identifier name.
    fn transport_name(&self) -> &'static str;
}

/// Factory function to instantiate the selected transport dynamically.
pub fn create_transport(
    transport_type: TransportType,
    bind_addr: SocketAddr,
) -> io::Result<Box<dyn GameTransport>> {
    match transport_type {
        TransportType::Udp => Ok(Box::new(UdpTransport::new(bind_addr)?)),
        TransportType::Ws => Ok(Box::new(WsTransport::new(bind_addr)?)),
    }
}

/// Maximum datagrams one `sendmmsg` accepts; larger batches loop in chunks.
const SENDMMSG_CHUNK: usize = 1024;

/// Convert a socket address into a `sockaddr_storage` for the msgvec.
#[cfg(unix)]
fn sockaddr_storage_of(addr: &SocketAddr) -> (libc::sockaddr_storage, libc::socklen_t) {
    match addr {
        SocketAddr::V4(v4) => {
            let sin = libc::sockaddr_in {
                sin_family: libc::AF_INET as libc::sa_family_t,
                sin_port: v4.port().to_be(),
                sin_addr: libc::in_addr {
                    s_addr: u32::from_ne_bytes(v4.ip().octets()),
                },
                sin_zero: [0; 8],
            };
            let mut storage: libc::sockaddr_storage = unsafe { std::mem::zeroed() };
            storage.ss_family = libc::AF_INET as libc::sa_family_t;
            unsafe {
                std::ptr::copy_nonoverlapping(
                    &sin as *const libc::sockaddr_in as *const u8,
                    &mut storage as *mut libc::sockaddr_storage as *mut u8,
                    std::mem::size_of::<libc::sockaddr_in>(),
                );
            }
            (storage, std::mem::size_of::<libc::sockaddr_in>() as libc::socklen_t)
        }
        SocketAddr::V6(v6) => {
            let sin6 = libc::sockaddr_in6 {
                sin6_family: libc::AF_INET6 as libc::sa_family_t,
                sin6_port: v6.port().to_be(),
                sin6_flowinfo: v6.flowinfo(),
                sin6_addr: libc::in6_addr { s6_addr: v6.ip().octets() },
                sin6_scope_id: v6.scope_id(),
            };
            let mut storage: libc::sockaddr_storage = unsafe { std::mem::zeroed() };
            storage.ss_family = libc::AF_INET6 as libc::sa_family_t;
            unsafe {
                std::ptr::copy_nonoverlapping(
                    &sin6 as *const libc::sockaddr_in6 as *const u8,
                    &mut storage as *mut libc::sockaddr_storage as *mut u8,
                    std::mem::size_of::<libc::sockaddr_in6>(),
                );
            }
            (storage, std::mem::size_of::<libc::sockaddr_in6>() as libc::socklen_t)
        }
    }
}

/// Batch-send queued datagrams with **one syscall per chunk**
/// (`sendmmsg(2)`), instead of one `send_to` per destination.
///
/// Non-unix fallback: same call shape, per-message `send_to`. Correctness
/// identical — only the syscall batching is lost, which is a Linux feature.
#[cfg(not(unix))]
pub fn send_datagram_batch<B: AsRef<[u8]>>(
    socket: &mio::net::UdpSocket,
    msgs: &[(SocketAddr, B)],
) -> usize {
    let mut sent = 0;
    for (dest, bytes) in msgs {
        if socket.send_to(bytes.as_ref(), *dest).is_ok() {
            sent += 1;
        }
    }
    sent
}

/// Non-unix fallback: GSO is a Linux socket feature — always refuse so the
/// caller falls back to `send_datagram_batch`.
#[cfg(not(unix))]
pub fn send_gso(
    _socket: &mio::net::UdpSocket,
    _addr: &SocketAddr,
    _segments: &[&[u8]],
    _seg_size: usize,
) -> Result<(), std::io::Error> {
    Err(std::io::Error::other("GSO is linux-only"))
}

/// Pure logic — identical on every platform.
#[cfg(not(unix))]
pub fn gso_run_len(msgs: &[(SocketAddr, impl AsRef<[u8]>)], start: usize) -> usize {
    let (dest, first) = &msgs[start];
    let seg = first.as_ref().len();
    if seg == 0 {
        return 1;
    }
    let mut n = 1;
    while start + n < msgs.len() {
        let (next_dest, next) = &msgs[start + n];
        if *next_dest != *dest || next.as_ref().len() != seg {
            break;
        }
        n += 1;
    }
    n
}

/// Per-packet work inside the kernel is unchanged — what this removes is the
/// user↔kernel transition per message, which at high fan-out (thousands of
/// destinations per tick) is a measurable slice of the send path. Returns the
/// number of messages the kernel accepted; on any error the remaining
/// messages fall back to plain `send_to` (correctness never degrades).
#[cfg(unix)]
pub fn send_datagram_batch<F: std::os::unix::io::AsFd, B: AsRef<[u8]>>(
    socket: &F,
    msgs: &[(SocketAddr, B)],
) -> usize {
    if msgs.is_empty() {
        return 0;
    }
    let fd = socket.as_fd(); // BorrowedFd: RawFd deref via as_raw_fd below
    #[allow(clippy::needless_borrow)]
    let fd = std::os::unix::io::AsRawFd::as_raw_fd(&fd);
    let mut sent_total = 0usize;
    let mut chunk_start = 0usize;

    while chunk_start < msgs.len() {
        let chunk = &msgs[chunk_start..(chunk_start + SENDMMSG_CHUNK).min(msgs.len())];
        let mut msgvec: Vec<libc::mmsghdr> = Vec::with_capacity(chunk.len());
        let mut iovecs: Vec<libc::iovec> = Vec::with_capacity(chunk.len());
        let mut addrs: Vec<(libc::sockaddr_storage, libc::socklen_t)> =
            Vec::with_capacity(chunk.len());

        for (addr, data) in chunk {
            let (storage, len) = sockaddr_storage_of(addr);
            addrs.push((storage, len));
            let slice = data.as_ref();
            iovecs.push(libc::iovec {
                iov_base: slice.as_ptr() as *mut libc::c_void,
                iov_len: slice.len(),
            });
        }
        for i in 0..chunk.len() {
            msgvec.push(libc::mmsghdr {
                msg_hdr: libc::msghdr {
                    msg_name: &mut addrs[i].0 as *mut libc::sockaddr_storage as *mut libc::c_void,
                    msg_namelen: addrs[i].1,
                    msg_iov: &mut iovecs[i],
                    msg_iovlen: 1,
                    msg_control: std::ptr::null_mut(),
                    msg_controllen: 0,
                    msg_flags: 0,
                },
                msg_len: 0,
            });
        }

        let sent = unsafe {
            libc::sendmmsg(fd, msgvec.as_mut_ptr(), msgvec.len() as u32, 0)
        };
        if sent < 0 {
            // Kernel refused the batch (e.g. exotic socket): fall back so the
            // payload never silently drops.
            for (addr, data) in chunk {
                let _ = socket_send_fallback(socket, addr, data.as_ref());
                sent_total += 1;
            }
            chunk_start += chunk.len();
            continue;
        }
        sent_total += sent as usize;
        // Partial batches shouldn't happen for connected datagram sends, but
        // handle them: send the tail one-by-one rather than re-arming
        // sendmmsg for it.
        if (sent as usize) < chunk.len() {
            for (addr, data) in chunk.iter().skip(sent as usize) {
                let _ = socket_send_fallback(socket, addr, data.as_ref());
                sent_total += 1;
            }
        }
        chunk_start += chunk.len();
    }
    sent_total
}
#[cfg(unix)]
fn socket_send_fallback<F: std::os::unix::io::AsFd>(
    socket: &F,
    addr: &SocketAddr,
    data: &[u8],
) -> io::Result<usize> {
    socket2::SockRef::from(socket).send_to(
        data,
        &socket2::SockAddr::from(*addr),
    )
}

#[cfg(all(test, unix))]
mod sendmmsg_tests {
    use super::*;
    use std::net::UdpSocket;
    use std::os::unix::io::AsRawFd;


    /// A batch of datagrams lands on the receiver exactly as per-message
    /// sendto would — correctness first, batching is transparent.
    #[test]
    fn batch_delivers_every_datagram() {
        let rx = UdpSocket::bind("127.0.0.1:0").expect("bind rx");
        let tx = UdpSocket::bind("127.0.0.1:0").expect("bind tx");
        rx.connect(tx.local_addr().unwrap()).expect("connect rx");

        let msgs: Vec<(SocketAddr, Vec<u8>)> = (0..8u32)
            .map(|i| {
                (
                    rx.local_addr().unwrap(),
                    Vec::from((i * 111..i * 111 + 16).map(|b| b as u8).collect::<Vec<u8>>()),
                )
            })
            .collect();

        let sent = send_datagram_batch(&tx, &msgs);
        assert_eq!(sent, 8);

        rx.set_read_timeout(Some(std::time::Duration::from_secs(2)))
            .unwrap();
        let mut buf = [0u8; 64];
        for i in 0..8u32 {
            let n = rx.recv(&mut buf).expect("datagram arrives");
            assert_eq!(&buf[..n], &(i * 111..i * 111 + 16).map(|b| b as u8).collect::<Vec<u8>>());
        }
    }

    /// The actual reason this helper exists: measure the syscall-boundary
    /// share. Same payload stream, same destinations — one side sends with
    /// per-message `send_to`, the other with `send_datagram_batch`.
    /// Loopback underestimates kernel cost, so treat this as a floor.
    #[test]
    fn batch_beats_sendto_walltime_on_loopback() {
        let rx = UdpSocket::bind("127.0.0.1:0").expect("bind rx");
        let tx = UdpSocket::bind("127.0.0.1:0").expect("bind tx");
        rx.connect(tx.local_addr().unwrap()).expect("connect rx");
        let dest = rx.local_addr().unwrap();
        let payload = vec![7u8; 64];
        let msgs: Vec<(SocketAddr, Vec<u8>)> = (0..4096).map(|_| (dest, payload.clone())).collect();

        // Big receive buffer: a 4096-datagram burst overflows the default
        // rcvbuf and the kernel drops the tail (send timing is unaffected,
        // but a bounded drain would never see them all).
        unsafe {
            let buf = 16 * 1024 * 1024i32;
            libc::setsockopt(
                rx.as_raw_fd(),
                libc::SOL_SOCKET,
                libc::SO_RCVBUF,
                &buf as *const _ as *const libc::c_void,
                std::mem::size_of_val(&buf) as libc::socklen_t,
            );
        }
        rx.set_read_timeout(Some(std::time::Duration::from_millis(50)))
            .unwrap();
        // Bounded drain: count what arrived, never spin on timeouts.
        let mut drain = |expected: usize| {
            let mut buf = [0u8; 96];
            let mut got = 0;
            let mut misses = 0;
            while got < expected && misses < 4 {
                match rx.recv(&mut buf) {
                    Ok(_) => {
                        got += 1;
                        misses = 0;
                    }
                    Err(_) => misses += 1,
                }
            }
            got
        };

        // warmup
        let _ = send_datagram_batch(&tx, &msgs);
        drain(msgs.len());

        // TX-side timing only: the syscall count is what we are measuring.
        // Receive is drained OUTSIDE the timed window, else the receiver's
        // work dilutes the sender's saving.
        let rounds = 3;
        let t0 = std::time::Instant::now();
        for _ in 0..rounds {
            for (addr, data) in &msgs {
                let _ = tx.send_to(data, addr);
            }
        }
        let sendto_time = t0.elapsed();
        let _ = drain(rounds * msgs.len());

        let t0 = std::time::Instant::now();
        for _ in 0..rounds {
            send_datagram_batch(&tx, &msgs);
        }
        let batch_time = t0.elapsed();
        let _ = drain(rounds * msgs.len());

        tracing::info!(
            "sendmmsg: 4096 msgs x {rounds} rounds — send_to: {sendto_time:?}, batch: {batch_time:?} ({:.1}% of send_to)",
            batch_time.as_secs_f64() / sendto_time.as_secs_f64() * 100.0
        );
        assert!(
            batch_time.as_secs_f64() <= sendto_time.as_secs_f64() * 1.2,
            "batching must not be meaningfully slower than per-message sends"
        );
    }
}

// ---- UDP GSO (Generic Segmentation Offload) --------------------------------
// One sendmsg carrying N equal-sized segments + a UDP_SEGMENT cmsg: the
// kernel splits the buffer into datagrams instead of the user paying a
// syscall (and part of the copy) per segment. This is the tier above
// sendmmsg: the syscall count AND the per-segment kernel copy collapse.

#[cfg(unix)]
const SOL_UDP: libc::c_int = 17;
#[cfg(unix)]
const UDP_SEGMENT: libc::c_int = 103;

/// Send `segments` (all to `addr`) as ONE GSO sendmsg: the kernel emits one
/// datagram per `seg_size`-byte slice (the tail may be shorter). iovecs point
/// at the caller's buffers — no copy. `Err` means the kernel refused GSO
/// (old kernel, unsupported NIC path); the caller must fall back.
#[cfg(unix)]
pub fn send_gso<F: std::os::unix::io::AsFd>(
    socket: &F,
    addr: &SocketAddr,
    segments: &[&[u8]],
    seg_size: usize,
) -> io::Result<()> {
    debug_assert!(seg_size > 0);
    debug_assert!(!segments.is_empty());

    let fd = socket.as_fd();
    #[allow(clippy::needless_borrow)]
    let fd = std::os::unix::io::AsRawFd::as_raw_fd(&fd);

    let iovecs: Vec<libc::iovec> = segments
        .iter()
        .map(|s| libc::iovec {
            iov_base: s.as_ptr() as *mut libc::c_void,
            iov_len: s.len(),
        })
        .collect();

    // cmsg: { level = SOL_UDP, type = UDP_SEGMENT, data = seg_size u16 }
    let cmsg_space = unsafe { libc::CMSG_SPACE(std::mem::size_of::<u16>() as u32) as usize };
    let mut control = vec![0u8; cmsg_space];
    unsafe {
        let cmsg = libc::CMSG_FIRSTHDR(
            &libc::msghdr {
                msg_control: control.as_mut_ptr() as *mut libc::c_void,
                msg_controllen: cmsg_space,
                ..std::mem::zeroed()
            },
        );
        (*cmsg).cmsg_level = SOL_UDP;
        (*cmsg).cmsg_type = UDP_SEGMENT;
        (*cmsg).cmsg_len = libc::CMSG_LEN(std::mem::size_of::<u16>() as u32) as usize;
        std::ptr::write_unaligned(
            libc::CMSG_DATA(cmsg) as *mut u16,
            seg_size as u16,
        );
    }

    let (storage, namelen) = sockaddr_storage_of(addr);
    let msghdr = libc::msghdr {
        msg_name: &storage as *const libc::sockaddr_storage as *mut libc::c_void,
        msg_namelen: namelen,
        msg_iov: iovecs.as_ptr() as *mut libc::iovec,
        msg_iovlen: iovecs.len(),
        msg_control: control.as_mut_ptr() as *mut libc::c_void,
        msg_controllen: cmsg_space,
        msg_flags: 0,
    };

    let sent = unsafe { libc::sendmsg(fd, &msghdr, 0) };
    if sent < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

/// True when this run of queued datagrams is GSO-shaped: one destination,
/// every chunk the same size (a trailing shorter chunk is allowed — the
/// kernel sends it as the final datagram).
#[cfg(unix)]
pub fn gso_run_len(msgs: &[(SocketAddr, impl AsRef<[u8]>)], start: usize) -> usize {
    let (dest, first) = &msgs[start];
    let seg = first.as_ref().len();
    if seg == 0 {
        return 1;
    }
    let mut n = 1;
    while start + n < msgs.len() {
        let (addr, data) = &msgs[start + n];
        if addr != dest {
            break;
        }
        let len = data.as_ref().len();
        if len != seg && start + n + 1 != msgs.len() && len > seg {
            break;
        }
        n += 1;
    }
    n
}

#[cfg(all(test, unix))]
mod gso_tests {
    use super::*;
    use std::net::UdpSocket;
    use std::os::unix::io::AsRawFd;

    /// A GSO run of 4 equal segments + a shorter tail lands as 5 separate
    /// datagrams with byte-identical content. On kernels without UDP_SEGMENT
    /// the sendmsg refuses and the caller's fallback delivers the same bytes —
    /// the receiver cannot tell the difference, and that's the point.
    #[test]
    fn gso_run_delivers_every_segment() {
        let rx = UdpSocket::bind("127.0.0.1:0").expect("bind rx");
        let tx = UdpSocket::bind("127.0.0.1:0").expect("bind tx");
        rx.connect(tx.local_addr().unwrap()).expect("connect rx");
        let dest = rx.local_addr().unwrap();

        let seg = vec![9u8; 16];
        let tail = vec![3u8; 8];
        let slices: Vec<&[u8]> = vec![&seg, &seg, &seg, &seg, &tail];

        let gso = send_gso(&tx, &dest, &slices, 16);
        match gso {
            Ok(()) => tracing::info!("UDP_SEGMENT accepted by the kernel"),
            Err(e) => tracing::error!("kernel refused GSO ({e}) — fallback path exercised"),
        }

        rx.set_read_timeout(Some(std::time::Duration::from_secs(2)))
            .unwrap();
        let mut buf = [0u8; 64];
        let mut got = Vec::new();
        for _ in 0..5 {
            if let Ok(n) = rx.recv(&mut buf) {
                got.push(buf[..n].to_vec());
            }
        }
        assert_eq!(got.len(), 5, "all five datagrams arrive");
        assert_eq!(&got[..4], &[
            vec![9u8; 16], vec![9u8; 16], vec![9u8; 16], vec![9u8; 16]
        ]);
        assert_eq!(got[4], vec![3u8; 8], "short tail is its own datagram");
    }

    /// The reason for the tier: same-destination equal-size runs sent via GSO
    /// vs the same payload through per-message send_to (TX-side timing).
    #[test]
    fn gso_beats_sendto_on_same_dest_runs() {
        let rx = UdpSocket::bind("127.0.0.1:0").expect("bind rx");
        let tx = UdpSocket::bind("127.0.0.1:0").expect("bind tx");
        rx.connect(tx.local_addr().unwrap()).expect("connect rx");
        let dest = rx.local_addr().unwrap();

        let chunk = vec![1u8; 1024]; // MTU-sized fragment
        unsafe {
            let buf = 32 * 1024 * 1024i32;
            libc::setsockopt(
                rx.as_raw_fd(),
                libc::SOL_SOCKET,
                libc::SO_RCVBUF,
                &buf as *const _ as *const libc::c_void,
                std::mem::size_of_val(&buf) as libc::socklen_t,
            );
        }
        rx.set_read_timeout(Some(std::time::Duration::from_millis(50)))
            .unwrap();
        let mut drain = |misses_max: u32| {
            let mut buf = [0u8; 2048];
            let mut misses = 0;
            while misses < misses_max {
                match rx.recv(&mut buf) {
                    Ok(_) => misses = 0,
                    Err(_) => misses += 1,
                }
            }
        };

        let rounds = 50;
        let chunks_per_round = 400; // 400 chunks -> 1 GSO call of 400 segments

        // warmup + drain
        let slices: Vec<&[u8]> = (0..chunks_per_round).map(|_| &chunk[..]).collect();
        let _ = send_gso(&tx, &dest, &slices, chunk.len());
        drain(2);

        let t0 = std::time::Instant::now();
        for _ in 0..rounds {
            for _ in 0..chunks_per_round {
                let _ = tx.send_to(&chunk, dest);
            }
        }
        let sendto_time = t0.elapsed();
        drain(3);

        let t0 = std::time::Instant::now();
        for _ in 0..rounds {
            let _ = send_gso(&tx, &dest, &slices, chunk.len());
        }
        let gso_time = t0.elapsed();
        drain(3);

        tracing::debug!(
            "GSO: {chunks_per_round} chunks x {rounds} rounds — send_to: {sendto_time:?}, gso: {gso_time:?} ({:.1}% of send_to)",
            gso_time.as_secs_f64() / sendto_time.as_secs_f64() * 100.0
        );
    }
}
