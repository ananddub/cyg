//! # Raw UDP Transport
//!
//! High-frequency, connectionless UDP datagram transport with `SO_REUSEPORT`.
//! Zero connection handshake overhead, perfect for competitive action and volatile inputs.

use std::collections::HashMap;
use std::io;
use std::net::SocketAddr;
use std::time::{Duration, Instant};

use mio::net::UdpSocket;
use mio::{Events, Interest, Poll, Token};
use socket2::{Domain, Protocol, Socket, Type};

use super::{GameTransport, TransportEvent};
use crate::framing::{decode_frame, encode_frame, OpCode};

const UDP_SOCKET_TOKEN: Token = Token(0);

struct UdpPeer {
    client_id: String,
    last_seen: Instant,
}

pub struct UdpTransport {
    poll: Poll,
    events: Events,
    socket: UdpSocket,
    peers: HashMap<SocketAddr, UdpPeer>,
    peer_addrs: HashMap<String, SocketAddr>,
    bind_addr: SocketAddr,
    recv_buf: [u8; 65535],
    send_queue: Vec<(SocketAddr, Vec<u8>)>,
}

impl UdpTransport {
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
            UDP_SOCKET_TOKEN,
            Interest::READABLE | Interest::WRITABLE,
        )?;

        Ok(Self {
            poll,
            events,
            socket: mio_socket,
            peers: HashMap::new(),
            peer_addrs: HashMap::new(),
            bind_addr,
            recv_buf: [0u8; 65535],
            send_queue: Vec::new(),
        })
    }
}

impl GameTransport for UdpTransport {
    fn poll_events(&mut self, timeout: Duration) -> io::Result<Vec<TransportEvent>> {
        self.poll.poll(&mut self.events, Some(timeout))?;

        let mut events_out = Vec::new();

        for event in self.events.iter() {
            if event.token() == UDP_SOCKET_TOKEN && event.is_readable() {
                loop {
                    match self.socket.recv_from(&mut self.recv_buf) {
                        Ok((len, peer_addr)) => {
                            let now = Instant::now();
                            let client_id = self.peers.entry(peer_addr).or_insert_with(|| {
                                let id = format!("udp_{}", peer_addr.port());
                                events_out.push(TransportEvent::Connected {
                                    client_id: id.clone(),
                                    peer_addr,
                                });
                                UdpPeer {
                                    client_id: id,
                                    last_seen: now,
                                }
                            });
                            client_id.last_seen = now;
                            let cid = client_id.client_id.clone();
                            self.peer_addrs.insert(cid.clone(), peer_addr);

                            // Decode binary packet
                            let mut slice = self.recv_buf[..len].to_vec();
                            while let Some((opcode, payload)) = decode_frame(&mut slice) {
                                events_out.push(TransportEvent::Packet {
                                    client_id: cid.clone(),
                                    opcode,
                                    payload,
                                });
                            }
                        }
                        Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => break,
                        Err(e) => return Err(e),
                    }
                }
            }
        }

        // Clean up inactive UDP peers (timeout after 10s of silence)
        let now = Instant::now();
        let timeout_thresh = Duration::from_secs(10);
        let mut timed_out = Vec::new();

        self.peers.retain(|&addr, peer| {
            if now.duration_since(peer.last_seen) > timeout_thresh {
                timed_out.push((peer.client_id.clone(), addr));
                false
            } else {
                true
            }
        });

        for (cid, _addr) in timed_out {
            self.peer_addrs.remove(&cid);
            events_out.push(TransportEvent::Disconnected { client_id: cid });
        }

        Ok(events_out)
    }

    fn send_to(&mut self, client_id: &str, opcode: OpCode, payload: &[u8]) -> io::Result<()> {
        if let Some(&peer_addr) = self.peer_addrs.get(client_id) {
            let mut out = Vec::new();
            encode_frame(opcode, payload, &mut out);
            self.send_queue.push((peer_addr, out));
        }
        Ok(())
    }

    fn broadcast(&mut self, opcode: OpCode, payload: &[u8]) -> io::Result<()> {
        let mut out = Vec::new();
        encode_frame(opcode, payload, &mut out);
        for &peer_addr in self.peer_addrs.values() {
            self.send_queue.push((peer_addr, out.clone()));
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
        self.peers.len()
    }

    fn local_addr(&self) -> io::Result<SocketAddr> {
        Ok(self.bind_addr)
    }

    fn transport_name(&self) -> &'static str {
        "Raw UDP"
    }
}
