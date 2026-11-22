use std::collections::HashMap;
use std::io;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Instant;

use bytes::{Bytes, BytesMut};

use crate::chunk::{
    CHUNK_ACK_OP, CHUNK_SLICE_OP, ChunkReceiver, ChunkSender, DEFAULT_SLICE_SIZE, parse_ack,
    parse_slice, write_ack,
};
use crate::framing::{OpCode, encode_frame};
use mio::net::UdpSocket;
use mio::{Events, Interest, Poll, Token};
use socket2::{Domain, Protocol, Socket, Type};

mod transport;

const UDP_SOCKET_TOKEN: Token = Token(0);

/// Payloads above this size go through the chunk layer on the datagram path.
/// One lost UDP datagram can no longer destroy an entire join snapshot.
const CHUNK_THRESHOLD: usize = 1200;
/// At most this many fresh slices are emitted per pump, so a 256 KiB block
/// does not burst 256 datagrams into one event-loop turn.
const CHUNK_BURST: usize = 8;

struct ChunkOut {
    sender: ChunkSender,
    opcode: OpCode,
}

struct UdpPeer {
    client_id: Arc<str>,
    last_seen: Instant,
}

pub struct UdpTransport {
    poll: Poll,
    events: Events,
    socket: UdpSocket,
    peers: HashMap<SocketAddr, UdpPeer>,
    peer_addrs: HashMap<String, SocketAddr>,
    bind_addr: SocketAddr,
    recv_buf: BytesMut,
    send_queue: Vec<(SocketAddr, Bytes)>,

    sb: BytesMut,

    timed_out: Vec<(SocketAddr, Arc<str>)>,

    chunk_out: HashMap<String, ChunkOut>,
    chunk_in: HashMap<(SocketAddr, u16), ChunkReceiver>,
    chunk_seq: u16,
    started: Instant,
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
        let bind_addr = std_socket.local_addr()?;
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
            recv_buf: BytesMut::with_capacity(65536),
            send_queue: Vec::new(),
            sb: BytesMut::with_capacity(1024),
            timed_out: Vec::new(),
            chunk_out: HashMap::new(),
            chunk_in: HashMap::new(),
            chunk_seq: 0,
            started: Instant::now(),
        })
    }

    fn queue_frame(&mut self, addr: SocketAddr, opcode: OpCode, payload: &[u8]) {
        let mut sb = BytesMut::with_capacity(3 + payload.len());
        if encode_frame(opcode, payload, &mut sb).is_ok() {
            self.send_queue.push((addr, sb.freeze()));
        }
    }

    fn now_ms(&self) -> u64 {
        self.started.elapsed().as_millis() as u64
    }

    /// Start a chunked send for `client_id`, superseding any older block.
    /// Returns `false` when the block must take the ordinary datagram path.
    fn start_chunk(&mut self, client_id: &str, opcode: OpCode, payload: &[u8]) -> bool {
        let Some(&addr) = self.peer_addrs.get(client_id) else {
            return false;
        };
        let mut block = Vec::with_capacity(1 + payload.len());
        block.push(opcode as u8);
        block.extend_from_slice(payload);
        self.chunk_seq = self.chunk_seq.wrapping_add(1);
        if self.chunk_seq == 0 {
            self.chunk_seq = 1;
        }
        let Ok(sender) = ChunkSender::new(self.chunk_seq, block, DEFAULT_SLICE_SIZE) else {
            return false;
        };
        self.chunk_out.insert(
            client_id.to_string(),
            ChunkOut { sender, opcode },
        );
        self.pump_chunk(client_id, addr, CHUNK_BURST);
        true
    }

    fn pump_chunk(&mut self, client_id: &str, addr: SocketAddr, burst: usize) {
        let now = self.now_ms();
        let mut wires: Vec<Vec<u8>> = Vec::new();
        {
            let Some(out) = self.chunk_out.get_mut(client_id) else {
                return;
            };
            let mut wire = Vec::with_capacity(DEFAULT_SLICE_SIZE + 9);
            for _ in 0..burst {
                let Some(_index) = out.sender.next_wire(now, &mut wire) else {
                    break;
                };
                wires.push(wire.clone());
            }
            if out.sender.is_complete() {
                self.chunk_out.remove(client_id);
            }
        }
        for wire in wires {
            self.queue_frame(addr, OpCode::Custom, &wire);
        }
    }

    /// Feed one custom datagram through the chunk layer. Returns `Some((opcode,
    /// payload))` when a whole block completed, `None` when it was a slice/ack
    /// or was malformed.
    fn accept_chunk(
        &mut self,
        client_id: &str,
        addr: SocketAddr,
        payload: &[u8],
    ) -> Option<(OpCode, Vec<u8>)> {
        let Some(&op) = payload.first() else {
            return None;
        };
        if op == CHUNK_ACK_OP {
            let Ok(ack) = parse_ack(payload) else {
                return None;
            };
            if let Some(out) = self.chunk_out.get_mut(client_id) {
                let _ = out.sender.on_ack(&ack);
            }
            if self
                .chunk_out
                .get(client_id)
                .is_some_and(|out| out.sender.is_complete())
            {
                self.chunk_out.remove(client_id);
            }
            return None;
        }
        if op != CHUNK_SLICE_OP {
            return None;
        }
        let Ok(slice) = parse_slice(payload) else {
            return None;
        };
        let key = (addr, slice.chunk_id);
        let receiver = self.chunk_in.entry(key).or_default();
        let block = match receiver.accept(payload) {
            Ok(block) => block,
            Err(_) => {
                self.chunk_in.remove(&key);
                return None;
            }
        };
        let mut ack_wire = Vec::new();
        write_ack(&mut ack_wire, &receiver.ack());
        self.queue_frame(addr, OpCode::Custom, &ack_wire);
        let block = block?;
        self.chunk_in.remove(&key);
        let opcode = OpCode::from_u8(*block.first()?)?;
        Some((opcode, block[1..].to_vec()))
    }
}

#[cfg(test)]
mod tests {
    use std::net::UdpSocket;
    use std::time::{Duration, Instant};

    use super::*;
    use crate::chunk::{ChunkReceiver, parse_slice, write_ack};
    use crate::framing::iter_frames;
    use crate::transport::{GameTransport, TransportEvent};

    #[test]
    fn a_large_udp_snapshot_survives_dropped_slices_without_losing_the_block() {
        let mut server = UdpTransport::new("127.0.0.1:0".parse().unwrap()).unwrap();
        let server_addr = server.local_addr().unwrap();
        let client = UdpSocket::bind("127.0.0.1:0").unwrap();
        client
            .set_read_timeout(Some(Duration::from_millis(50)))
            .unwrap();

        // Join the server so it learns the client's address.
        let mut ping = Vec::new();
        encode_frame(OpCode::Ping, &[], &mut ping).unwrap();
        client.send_to(&ping, server_addr).unwrap();
        let mut events = Vec::new();
        let mut client_id = None;
        let deadline = Instant::now() + Duration::from_secs(2);
        while client_id.is_none() && Instant::now() < deadline {
            server
                .poll_events(Duration::from_millis(20), &mut events)
                .unwrap();
            client_id = events.iter().find_map(|event| match event {
                TransportEvent::Connected { client_id, .. } => Some(client_id.clone()),
                _ => None,
            });
        }
        let client_id = client_id.expect("server saw the client");

        let payload: Vec<u8> = (0..20_000).map(|i| (i % 251) as u8).collect();
        server
            .send_to(&client_id, OpCode::StateSnapshot, &payload)
            .unwrap();
        server.flush().unwrap();

        let mut receiver = ChunkReceiver::new();
        let mut assembled = None;
        let mut datagrams = 0u64;
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut buf = vec![0u8; 2048];
        while assembled.is_none() && Instant::now() < deadline {
            server
                .poll_events(Duration::from_millis(20), &mut events)
                .unwrap();
            server.flush().unwrap();
            loop {
                let (len, _) = match client.recv_from(&mut buf) {
                    Ok(pair) => pair,
                    Err(_) => break,
                };
                datagrams += 1;
                // Deterministic 1-in-11 loss: the transport must retry only
                // the missing slices, not the whole snapshot.
                if datagrams % 11 == 0 {
                    continue;
                }
                for (opcode, body) in iter_frames(&buf[..len]) {
                    if opcode != OpCode::Custom || parse_slice(body).is_err() {
                        continue;
                    }
                    assembled = receiver.accept(body).unwrap();
                    let mut ack = Vec::new();
                    write_ack(&mut ack, &receiver.ack());
                    let mut frame = Vec::new();
                    encode_frame(OpCode::Custom, &ack, &mut frame).unwrap();
                    client.send_to(&frame, server_addr).unwrap();
                }
            }
        }
        let assembled = assembled.expect("block reassembled");
        assert_eq!(&assembled[1..], payload.as_slice());
        assert!(datagrams > 20, "snapshot really was chunked");
    }
    #[test]
    fn a_large_udp_snapshot_converges_over_150ms_rtt_and_5_percent_loss() {
        use std::collections::VecDeque;
        use std::sync::Arc;
        use std::sync::atomic::{AtomicBool, Ordering};
        use std::thread;

        let mut server = UdpTransport::new("127.0.0.1:0".parse().unwrap()).unwrap();
        let server_addr = server.local_addr().unwrap();
        let proxy = UdpSocket::bind("127.0.0.1:0").unwrap();
        proxy.set_nonblocking(true).unwrap();
        let proxy_addr = proxy.local_addr().unwrap();
        let client = UdpSocket::bind("127.0.0.1:0").unwrap();
        client
            .set_read_timeout(Some(Duration::from_millis(20)))
            .unwrap();

        // A real two-way proxy: 75 ms each way, every 20th datagram dropped.
        let stop = Arc::new(AtomicBool::new(false));
        let proxy_stop = stop.clone();
        let proxy_thread = thread::spawn(move || {
            let mut queue: VecDeque<(Instant, Vec<u8>, SocketAddr)> = VecDeque::new();
            let mut client_addr = None;
            let mut seen = 0u64;
            let mut buf = vec![0u8; 4096];
            while !proxy_stop.load(Ordering::Relaxed) {
                loop {
                    let (len, from) = match proxy.recv_from(&mut buf) {
                        Ok(pair) => pair,
                        Err(_) => break,
                    };
                    let to = if from == server_addr {
                        match client_addr {
                            Some(addr) => addr,
                            None => continue,
                        }
                    } else {
                        client_addr = Some(from);
                        server_addr
                    };
                    seen += 1;
                    if seen % 20 != 0 {
                        queue.push_back((
                            Instant::now() + Duration::from_millis(75),
                            buf[..len].to_vec(),
                            to,
                        ));
                    }
                }
                let now = Instant::now();
                while queue.front().is_some_and(|(at, _, _)| *at <= now) {
                    let (_, data, to) = queue.pop_front().unwrap();
                    let _ = proxy.send_to(&data, to);
                }
                thread::sleep(Duration::from_millis(1));
            }
        });

        // Join through the proxy; the server sees the proxy's address as the peer.
        let mut ping = Vec::new();
        encode_frame(OpCode::Ping, &[], &mut ping).unwrap();
        client.send_to(&ping, proxy_addr).unwrap();
        let mut events = Vec::new();
        let mut client_id = None;
        let deadline = Instant::now() + Duration::from_secs(3);
        while client_id.is_none() && Instant::now() < deadline {
            server
                .poll_events(Duration::from_millis(20), &mut events)
                .unwrap();
            client_id = events.iter().find_map(|event| match event {
                TransportEvent::Connected { client_id, .. } => Some(client_id.clone()),
                _ => None,
            });
        }
        let client_id = client_id.expect("server saw the proxied client");

        let payload: Vec<u8> = (0..20_000).map(|i| (i % 251) as u8).collect();
        server
            .send_to(&client_id, OpCode::StateSnapshot, &payload)
            .unwrap();
        server.flush().unwrap();

        let mut receiver = ChunkReceiver::new();
        let mut assembled = None;
        let mut datagrams = 0u64;
        let deadline = Instant::now() + Duration::from_secs(8);
        let mut buf = vec![0u8; 2048];
        while assembled.is_none() && Instant::now() < deadline {
            server
                .poll_events(Duration::from_millis(20), &mut events)
                .unwrap();
            server.flush().unwrap();
            loop {
                let (len, _) = match client.recv_from(&mut buf) {
                    Ok(pair) => pair,
                    Err(_) => break,
                };
                datagrams += 1;
                for (opcode, body) in iter_frames(&buf[..len]) {
                    if opcode != OpCode::Custom || parse_slice(body).is_err() {
                        continue;
                    }
                    assembled = receiver.accept(body).unwrap();
                    let mut ack = Vec::new();
                    write_ack(&mut ack, &receiver.ack());
                    let mut frame = Vec::new();
                    encode_frame(OpCode::Custom, &ack, &mut frame).unwrap();
                    client.send_to(&frame, proxy_addr).unwrap();
                }
            }
        }

        stop.store(true, Ordering::Relaxed);
        proxy_thread.join().unwrap();
        let assembled = assembled.expect("20 KB block reassembled over a lossy 150 ms link");
        assert_eq!(&assembled[1..], payload.as_slice());
        assert!(datagrams > 20, "block really was chunked and retransmitted");
    }

}
