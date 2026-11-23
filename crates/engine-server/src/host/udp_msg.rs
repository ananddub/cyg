use std::net::SocketAddr;
use std::sync::Arc;

use crate::host::*;

impl UdpLane {
    pub(crate) fn udp_reliable_msg(
        &mut self,
        udp_sock: &Arc<std::net::UdpSocket>,
        peer_addr: SocketAddr,
        seq: u16,
        payload: &[u8],
        ctx: &mut UdpCtx,
    ) {
        let seen = self.udp_rxd_seq.entry(peer_addr).or_default();
        let fresh = seen.insert(seq);
        if seen.len() > RUDP_DEDUPE_SPAN {
            seen.retain(|s| engine_webrtc_protocol::is_newer(seq, *s) || *s == seq);
        }
        let (cum, ranges) = rxd_summary(seen);
        netcode::encode_reliable_ack_sack(cum, &ranges, &mut self.rudp_scratch);
        let _ = udp_sock.send_to(&self.rudp_scratch, peer_addr);
        if !fresh {
            return;
        }
        let history = EventHistory::new(ctx.bus, ctx.game_cfg);
        let Some(room_name) = self.peer_room(peer_addr).map(str::to_owned) else {
            tracing::error!(
                "[server-udp] ⛔ reliable frame from unseated peer {peer_addr}; dropping"
            );
            return;
        };
        let room = match ctx.server.get_or_create(&room_name, b"") {
            Ok(room) => room,
            Err(_) => return,
        };
        room.lifecycle.empty_since = None;
        if let Some(report) = engine_webrtc_protocol::parse_checksum(payload) {
            if let Err(desync) = room
                .rollback_buffer
                .verify_sync(report.tick, report.checksum)
            {
                ctx.recorder.desyncs += 1;
                tracing::warn!(
                    "[server-udp] ⚠️ desync in room '{}': {peer_addr} disagrees at tick {} (host {:016x}, client {:016x})",
                    room_name,
                    desync.tick,
                    desync.expected_checksum,
                    desync.client_checksum
                );
                history.record_desync(&room_name, 0, &desync);
            }
        } else if let Some(ref mut wasm) = room.wasm {
            if let Ok(ptr) = wasm.on_alloc(payload.len() as i32) {
                let _ = wasm.write_memory(ptr as u32, payload);
                let _ = wasm.on_input(0, ptr, payload.len() as i32);
            }
        }
        flush_room_outbox(room, ctx.transport, &room_name, &history);
    }

    pub(crate) fn udp_room_packet(
        &mut self,
        udp_sock: &Arc<std::net::UdpSocket>,
        peer_addr: SocketAddr,
        data: &[u8],
        ctx: &mut UdpCtx,
    ) {
        let is_spec_handshake = data.len() == 5 && data[0] == 0xFF && &data[1..5] == b"SPEC";

        if !data.is_empty()
            && (data[0] == netcode::CYG_WIRE_LEAVE_ROOM || data[0] == netcode::CYG_OP_LEAVE_ROOM)
        {
            if let Some((id, sess, _)) = self.udp_peers.remove(&peer_addr) {
                let room_name = self
                    .udp_rooms
                    .remove(&peer_addr)
                    .unwrap_or_else(|| ctx.cfg.name.clone());
                tracing::info!(
                    "[server-udp] {peer_addr} left room '{room_name}' via UDP (session {sess})"
                );
                self.udp_input_rates.remove(&peer_addr);
                self.udp_reliable.remove(&peer_addr);
                self.udp_rxd_seq.remove(&peer_addr);
                if let Some(room) = ctx.server.rooms.get_mut(room_name.as_str()) {
                    room.sessions.remove(sess.as_str());
                    room.schema_sent.remove(sess.as_str());
                    if id > 0 {
                        room.remove_raw_player(id);
                    }
                }
            }
            return;
        }

        if self.udp_peers.contains_key(&peer_addr) {
            self.udp_existing(udp_sock, peer_addr, data, is_spec_handshake, ctx);
            return;
        }
        if ctx.bus.is_draining() {
            tracing::info!("[server-udp] ⛔ {peer_addr} refused: server is closing");
            return;
        }
        // Every new raw-UDP peer, spectators included, must present a seat
        // first. A tokenless `SPEC` upgrade was the old bypass; it is gone.
        self.udp_join(udp_sock, peer_addr, data, ctx);
    }
}
