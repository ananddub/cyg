//! # QUIC Transport
//!
//! Multiplexed QUIC transport supporting unreliable datagrams (player inputs)
//! and reliable streams (state snapshots and reliable RPC).

use std::collections::HashMap;
use std::io;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use bytes::{Bytes, BytesMut};

use mio::net::UdpSocket;
use mio::{Events, Interest, Poll, Token};
use socket2::{Domain, Protocol, Socket, Type};

use super::{GameTransport, TransportEvent};
use crate::framing::{encode_frame, iter_frame_ranges, OpCode};

const QUIC_SOCKET_TOKEN: Token = Token(0);

struct QuicSession {
    session_id: Arc<str>,
    last_active: Instant,
}

pub struct QuicTransport {
    poll: Poll,
    events: Events,
    socket: UdpSocket,
    sessions: HashMap<SocketAddr, QuicSession>,
    session_addrs: HashMap<String, SocketAddr>,
    bind_addr: SocketAddr,
    recv_buf: BytesMut,
    send_queue: Vec<(SocketAddr, Bytes)>,
    /// Encode scratch; `BytesMut` reuses its allocation across sends.
    sb: BytesMut,
    /// Timeouts collected during the sweep; persists across polls. Carries the
    /// session's `Arc<str>` id (a refcount bump, not a string copy).
    timed_out: Vec<(SocketAddr, Arc<str>)>,
}

impl QuicTransport {
    pub fn new(bind_addr: SocketAddr) -> io::Result<Self> {
        let poll = Poll::new()?;
        let events = Events::with_capacity(1024);

        let socket = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP))?;
        socket.set_reuse_address(true)?;
        #[cfg(all(unix, not(target_os = "solaris"), not(target_os = "illumos")))]
        socket.set_reuse_port(true)?;
        socket.set_nonblocking(true)?;

        let sock_addr = bind_addr.into();
        socket.bind(&sock_addr)?;

        let std_socket: std::net::UdpSocket = socket.into();
        let mut mio_socket = UdpSocket::from_std(std_socket);

        poll.registry().register(
            &mut mio_socket,
            QUIC_SOCKET_TOKEN,
            Interest::READABLE | Interest::WRITABLE,
        )?;

        Ok(Self {
            poll,
            events,
            socket: mio_socket,
            sessions: HashMap::new(),
            session_addrs: HashMap::new(),
            bind_addr,
            recv_buf: BytesMut::with_capacity(65536),
            send_queue: Vec::new(),
            sb: BytesMut::with_capacity(1024),
            timed_out: Vec::new(),
        })
    }
}

impl GameTransport for QuicTransport {
    fn poll_events(
        &mut self,
        timeout: Duration,
        events_out: &mut Vec<TransportEvent>,
    ) -> io::Result<()> {
        self.poll.poll(&mut self.events, Some(timeout))?;

        for event in self.events.iter() {
            if event.token() == QUIC_SOCKET_TOKEN && event.is_readable() {
                loop {
                    // Receive straight into the BytesMut; frames inside each
                    // datagram are zero-copy `Bytes` slices.
                    let (len, peer_addr) = match crate::bufio::recv_udp(
                        &mut self.socket,
                        &mut self.recv_buf,
                        65535,
                    )? {
                        Some(r) => r,
                        None => break,
                    };
                    let datagram = self.recv_buf.split_to(len).freeze();
                    let now = Instant::now();
                    let sid: Arc<str> = match self.sessions.get_mut(&peer_addr) {
                        Some(sess) => {
                            sess.last_active = now;
                            sess.session_id.clone()
                        }
                        None => {
                            let id: Arc<str> = format!("quic_{}", peer_addr.port()).into();
                            events_out.push(TransportEvent::Connected {
                                client_id: id.clone(),
                                peer_addr,
                            });
                            self.session_addrs.insert(id.to_string(), peer_addr);
                            self.sessions.insert(peer_addr, QuicSession {
                                session_id: id.clone(),
                                last_active: now,
                            });
                            id
                        }
                    };
                    for (opcode, range) in iter_frame_ranges(&datagram) {
                        events_out.push(TransportEvent::Packet {
                            client_id: sid.clone(),
                            opcode,
                            payload: datagram.slice(range),
                        });
                    }
                }
            }
        }

        // Clean up dead sessions after 15s of silence
        let now = Instant::now();
        let timeout_thresh = Duration::from_secs(15);
        let (timed_out, sessions) = (&mut self.timed_out, &mut self.sessions);
        sessions.retain(|&addr, sess| {
            if now.duration_since(sess.last_active) > timeout_thresh {
                timed_out.push((addr, sess.session_id.clone()));
                false
            } else {
                true
            }
        });

        for (_addr, sid) in timed_out.drain(..) {
            self.session_addrs.remove(&*sid);
            events_out.push(TransportEvent::Disconnected { client_id: sid });
        }

        Ok(())
    }

    fn send_to(&mut self, client_id: &str, opcode: OpCode, payload: &[u8]) -> io::Result<()> {
        if let Some(&peer_addr) = self.session_addrs.get(client_id) {
            self.sb.reserve(3 + payload.len());
            encode_frame(opcode, payload, &mut self.sb);
            let frame = self.sb.split().freeze();
            self.send_queue.push((peer_addr, frame));
        }
        Ok(())
    }

    fn broadcast(&mut self, opcode: OpCode, payload: &[u8]) -> io::Result<()> {
        self.sb.reserve(3 + payload.len());
        encode_frame(opcode, payload, &mut self.sb);
        let frame: Bytes = self.sb.split().freeze();
        let (send_queue, session_addrs) = (&mut self.send_queue, &self.session_addrs);
        for &peer_addr in session_addrs.values() {
            send_queue.push((peer_addr, frame.clone()));
        }
        Ok(())
    }

    fn flush(&mut self) -> io::Result<()> {
        for (peer, data) in self.send_queue.drain(..) {
            let _ = self.socket.send_to(&data, peer);
        }
        Ok(())
    }

    fn ccu(&self) -> usize {
        self.sessions.len()
    }

    fn local_addr(&self) -> io::Result<SocketAddr> {
        Ok(self.bind_addr)
    }

    fn transport_name(&self) -> &'static str {
        "QUIC (Datagram & Streams)"
    }
}
