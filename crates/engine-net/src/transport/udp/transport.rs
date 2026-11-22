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

        let ready: Vec<bool> = self
            .events
            .iter()
            .map(|event| event.token() == UDP_SOCKET_TOKEN && event.is_readable())
            .collect();
        for readable in ready {
            if readable {
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
                        if opcode == OpCode::Custom {
                            if let Some((opcode, payload)) =
                                self.accept_chunk(&cid, peer_addr, &datagram[range])
                            {
                                events_out.push(TransportEvent::Packet {
                                    client_id: cid.clone(),
                                    opcode,
                                    payload: payload.into(),
                                });
                            }
                            continue;
                        }
                        events_out.push(TransportEvent::Packet {
                            client_id: cid.clone(),
                            opcode,
                            payload: datagram.slice(range),
                        });
                    }
                }
            }
        }

        // Fresh slices and missing-slice retries for every in-flight block.
        let chunks: Vec<(String, SocketAddr)> = self
            .chunk_out
            .keys()
            .filter_map(|id| self.peer_addrs.get(id).map(|addr| (id.clone(), *addr)))
            .collect();
        for (client_id, addr) in chunks {
            self.pump_chunk(&client_id, addr, super::CHUNK_BURST);
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
        if payload.len() > super::CHUNK_THRESHOLD
            && matches!(
                opcode,
                OpCode::StateSnapshot
                    | OpCode::StateDelta
                    | OpCode::RoomDataBytes
                    | OpCode::RoomResponse
            )
            && self.start_chunk(client_id, opcode, payload)
        {
            return Ok(());
        }
        if let Some(&peer_addr) = self.peer_addrs.get(client_id) {
            self.sb.reserve(3 + payload.len());
            encode_frame(opcode, payload, &mut self.sb);
            let frame = self.sb.split().freeze();
            self.send_queue.push((peer_addr, frame));
        }
        Ok(())
    }

    fn broadcast(&mut self, opcode: OpCode, payload: &[u8]) -> io::Result<()> {
        if payload.len() > super::CHUNK_THRESHOLD
            && matches!(
                opcode,
                OpCode::StateSnapshot
                    | OpCode::StateDelta
                    | OpCode::RoomDataBytes
                    | OpCode::RoomResponse
            )
        {
            let ids: Vec<String> = self.peer_addrs.keys().cloned().collect();
            if !ids.is_empty()
                && ids
                    .iter()
                    .all(|id| self.start_chunk(id, opcode, payload))
            {
                return Ok(());
            }
        }
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
