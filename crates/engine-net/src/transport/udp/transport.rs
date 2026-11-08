use std::io;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use bytes::Bytes;

use super::{UDP_SOCKET_TOKEN, UdpPeer, UdpTransport};
use crate::framing::{OpCode, encode_frame, iter_frame_ranges};
use crate::transport::{GameTransport, TransportEvent};

impl GameTransport for UdpTransport {
    fn poll_events(
        &mut self,
        timeout: Duration,
        events_out: &mut Vec<TransportEvent>,
    ) -> io::Result<()> {
        self.poll.poll(&mut self.events, Some(timeout))?;

        for event in self.events.iter() {
            if event.token() == UDP_SOCKET_TOKEN && event.is_readable() {
                loop {
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
                    let cid: Arc<str> = match self.peers.get_mut(&peer_addr) {
                        Some(peer) => {
                            peer.last_seen = now;
                            peer.client_id.clone()
                        }
                        None => {
                            let id: Arc<str> = format!("udp_{}", peer_addr.port()).into();
                            events_out.push(TransportEvent::Connected {
                                client_id: id.clone(),
                                peer_addr,
                            });
                            self.peer_addrs.insert(id.to_string(), peer_addr);
                            self.peers.insert(
                                peer_addr,
                                UdpPeer {
                                    client_id: id.clone(),
                                    last_seen: now,
                                },
                            );
                            id
                        }
                    };
                    for (opcode, range) in iter_frame_ranges(&datagram) {
                        events_out.push(TransportEvent::Packet {
                            client_id: cid.clone(),
                            opcode,
                            payload: datagram.slice(range),
                        });
                    }
                }
            }
        }

        let now = Instant::now();
        let timeout_thresh = Duration::from_secs(10);
        let (timed_out, peers) = (&mut self.timed_out, &mut self.peers);
        peers.retain(|&addr, peer| {
            if now.duration_since(peer.last_seen) > timeout_thresh {
                timed_out.push((addr, peer.client_id.clone()));
                false
            } else {
                true
            }
        });

        for (_addr, cid) in timed_out.drain(..) {
            self.peer_addrs.remove(&*cid);
            events_out.push(TransportEvent::Disconnected { client_id: cid });
        }

        Ok(())
    }

    fn send_to(&mut self, client_id: &str, opcode: OpCode, payload: &[u8]) -> io::Result<()> {
        if let Some(&peer_addr) = self.peer_addrs.get(client_id) {
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
        let (send_queue, peer_addrs) = (&mut self.send_queue, &self.peer_addrs);
        for &peer_addr in peer_addrs.values() {
            send_queue.push((peer_addr, frame.clone()));
        }
        Ok(())
    }

    fn flush(&mut self) -> io::Result<()> {
        let batch: Vec<(SocketAddr, bytes::Bytes)> = self.send_queue.drain(..).collect();
        crate::transport::send_datagram_batch(&self.socket, &batch);
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
