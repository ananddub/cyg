use std::net::SocketAddr;

use crate::host::*;

impl UdpLane {
    pub(crate) fn run(&mut self, ctx: &mut UdpCtx) {
        let Some(udp_sock) = self.udp_sock.clone() else {
            return;
        };
        let mut udp_buf = [0u8; 4096];
        while let Ok((amt, peer_addr)) = udp_sock.recv_from(&mut udp_buf) {
            let data = &udp_buf[..amt];
            self.wire_bytes_in += amt as u64;

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

    pub(crate) fn pump(&mut self) {
        let Some(udp_sock) = self.udp_sock.clone() else {
            return;
        };
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
