use std::net::SocketAddr;

use engine_net::chunk::{
    CHUNK_ACK_OP, CHUNK_SLICE_OP, ChunkReceiver, ChunkSender, DEFAULT_SLICE_SIZE, MAX_BLOCK,
    parse_ack, parse_slice, write_ack,
};

use crate::host::*;

const CHUNK_THRESHOLD: usize = 1200;
const CHUNK_BURST: usize = 8;

impl UdpLane {
    pub(crate) fn run(&mut self, ctx: &mut UdpCtx) {
        let Some(udp_sock) = self.udp_sock.clone() else {
            return;
        };
        let mut udp_buf = [0u8; 4096];
        while let Ok((amt, peer_addr)) = udp_sock.recv_from(&mut udp_buf) {
            let data = &udp_buf[..amt];
            self.wire_bytes_in += amt as u64;

            if let Some(&op) = data.first()
                && op == CHUNK_ACK_OP
            {
                self.udp_chunk_ack(peer_addr, data);
                continue;
            }
            if let Some(&op) = data.first()
                && op == CHUNK_SLICE_OP
            {
                if let Some(block) = self.udp_chunk_slice(&udp_sock, peer_addr, data) {
                    // The reassembled block is the original frame: opcode first.
                    self.udp_room_packet(&udp_sock, peer_addr, &block, ctx);
                }
                continue;
            }

            if data.len() >= 3 && data[0] == netcode::CYG_WIRE_RELIABLE_ACK {
                if let Some((cum, ranges)) = netcode::decode_reliable_ack(data) {
                    if let Some(lane) = self.udp_reliable.get_mut(&peer_addr) {
                        lane.ack_selective(cum, &ranges, mono_ms());
                    }
                }
                continue;
            }
            if let Some((seq, payload)) = netcode::decode_reliable_msg(data) {
                self.udp_reliable_msg(&udp_sock, peer_addr, seq, payload, ctx);
                continue;
            }
            self.udp_room_packet(&udp_sock, peer_addr, data, ctx);
        }
    }

    /// Queue a block for chunked delivery. Returns false when the block must
    /// take the ordinary datagram path (too large, empty, or no peer).
    pub(crate) fn start_chunk(
        &mut self,
        peer: SocketAddr,
        opcode: u8,
        payload: &[u8],
    ) -> bool {
        if payload.is_empty() || payload.len() + 1 > MAX_BLOCK {
            return false;
        }
        let mut block = Vec::with_capacity(1 + payload.len());
        block.push(opcode);
        block.extend_from_slice(payload);
        self.udp_chunk_seq = self.udp_chunk_seq.wrapping_add(1);
        if self.udp_chunk_seq == 0 {
            self.udp_chunk_seq = 1;
        }
        let Ok(sender) = ChunkSender::new(self.udp_chunk_seq, block, DEFAULT_SLICE_SIZE) else {
            return false;
        };
        self.udp_chunks
            .insert(peer, UdpChunkOut { sender, opcode });
        self.pump_chunks();
        true
    }

    fn pump_chunks(&mut self) {
        let Some(udp_sock) = self.udp_sock.clone() else {
            return;
        };
        let now = mono_ms();
        let mut sends: Vec<(SocketAddr, Vec<u8>)> = Vec::new();
        for (peer, out) in self.udp_chunks.iter_mut() {
            let mut wire = Vec::with_capacity(DEFAULT_SLICE_SIZE + 9);
            for _ in 0..CHUNK_BURST {
                if out.sender.next_wire(now, &mut wire).is_none() {
                    break;
                }
                sends.push((*peer, wire.clone()));
            }
        }
        for (peer, wire) in sends {
            if udp_sock.send_to(&wire, peer).is_ok() {
                self.wire_bytes_out += wire.len() as u64;
                self.wire_fragments += 1;
            }
        }
        self.udp_chunks.retain(|_, out| !out.sender.is_complete());
    }

    fn udp_chunk_ack(&mut self, peer: SocketAddr, data: &[u8]) {
        let Ok(ack) = parse_ack(data) else {
            return;
        };
        if let Some(out) = self.udp_chunks.get_mut(&peer) {
            let _ = out.sender.on_ack(&ack);
        }
        if self
            .udp_chunks
            .get(&peer)
            .is_some_and(|out| out.sender.is_complete())
        {
            self.udp_chunks.remove(&peer);
        }
    }

    fn udp_chunk_slice(
        &mut self,
        udp_sock: &Arc<std::net::UdpSocket>,
        peer: SocketAddr,
        data: &[u8],
    ) -> Option<Vec<u8>> {
        let Ok(slice) = parse_slice(data) else {
            return None;
        };
        let receiver = self.udp_chunk_in.entry(peer).or_default();
        let block = match receiver.accept(data) {
            Ok(block) => block,
            Err(_) => {
                self.udp_chunk_in.remove(&peer);
                return None;
            }
        };
        let mut ack = Vec::new();
        write_ack(&mut ack, &receiver.ack());
        let _ = udp_sock.send_to(&ack, peer);
        if block.is_some() {
            self.udp_chunk_in.remove(&peer);
        }
        let _ = slice.chunk_id;
        block
    }

    pub(crate) fn pump(&mut self) {
        let Some(udp_sock) = self.udp_sock.clone() else {
            return;
        };
        self.pump_chunks();
        let now = mono_ms();
        let due: Vec<SocketAddr> = self
            .udp_reliable
            .iter()
            .filter(|(_, lane)| lane.inflight() > 0)
            .map(|(a, _)| *a)
            .collect();
        for addr in due {
            if let Some(lane) = self.udp_reliable.get_mut(&addr) {
                lane.pump(now, |frame| udp_sock.send_to(frame, addr).is_ok());
            }
        }
    }

    pub(crate) fn cleanup(&mut self, server: &mut GameServer, cfg: &GameConfig) {
        let now = Instant::now();
        self.timed_out_udp.clear();
        let idle_ms = cfg.lifecycle.peer_idle_ms;
        let timed_out = &mut self.timed_out_udp;
        self.udp_peers.retain(|&addr, (id, sess, last_seen)| {
            if now.duration_since(*last_seen) > Duration::from_millis(idle_ms) {
                timed_out.push((addr, *id, sess.clone()));
                false
            } else {
                true
            }
        });
        for (addr, id, sess) in self.timed_out_udp.drain(..) {
            tracing::info!("[server-udp] disconnected (timeout): {addr}");
            self.udp_input_rates.remove(&addr);
            if let Some(room) = server.rooms.get_mut(cfg.name.as_str()) {
                room.sessions.remove(sess.as_str());
                room.schema_sent.remove(sess.as_str());
                if id > 0 {
                    room.remove_raw_player(id);
                }
            }
        }
    }
}
