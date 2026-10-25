//! # Wire transports
//!
//! One folder per transport, plus the [`GameTransport`] trait that
//! [`create_transport`] selects through [`TransportType`]:
//!
//! - `udp`: raw non-blocking length-prefixed datagrams
//! - `ws`: the OpenOxide/Colyseus WebSocket wire protocol (the browser plane)
//! - `webrtc`: sans-IO `str0m` DataChannels and their channel contract
//!
//! Only `udp` and `ws` implement [`GameTransport`]; the WebRTC server is driven
//! directly by the game loop because what it moves is framed deltas, not packets.
//! Either way the simulation above this module does not know which wire a client
//! reached the room on.

pub mod udp;
pub mod webrtc;
pub mod ws;

use std::io;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use bytes::Bytes;

use crate::framing::OpCode;

pub use udp::UdpTransport;
pub use webrtc::{WebRtcServer, WebRtcServerEvent, WebRtcTransport};
pub use ws::WsTransport;

/// Supported transport implementations that sit behind [`GameTransport`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransportType {
    /// Raw high-frequency non-blocking UDP datagrams.
    Udp,
    /// WebSocket with OpenOxide SDK wire-protocol compatibility (zero client migration).
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

        // TX-side timing only: the syscall count is what we are measuring,
        // receiver-side drops under burst don't change it.
        let rounds = 3;
        let t0 = std::time::Instant::now();
        for _ in 0..rounds {
            for (addr, data) in &msgs {
                let _ = tx.send_to(data, addr);
            }
            let _ = drain(msgs.len());
        }
        let sendto_time = t0.elapsed();

        let t0 = std::time::Instant::now();
        for _ in 0..rounds {
            send_datagram_batch(&tx, &msgs);
            let _ = drain(msgs.len());
        }
        let batch_time = t0.elapsed();

        println!(
            "sendmmsg: 4096 msgs x {rounds} rounds — send_to: {sendto_time:?}, batch: {batch_time:?} ({:.1}% of send_to)",
            batch_time.as_secs_f64() / sendto_time.as_secs_f64() * 100.0
        );
        assert!(
            batch_time.as_secs_f64() <= sendto_time.as_secs_f64() * 1.2,
            "batching must not be meaningfully slower than per-message sends"
        );
    }
}
