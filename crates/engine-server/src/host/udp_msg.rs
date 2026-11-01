use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Instant;

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
        let room = match ctx.server.get_or_create(&ctx.cfg.name) {
            Ok(room) => room,
            Err(_) => return,
        };
        room.empty_since = None;
        if let Some(report) = engine_webrtc_protocol::parse_checksum(payload) {
            if let Err(desync) = room
                .rollback_buffer
                .verify_sync(report.tick, report.checksum)
            {
                ctx.recorder.desyncs += 1;
                tracing::warn!(
                    "[server-udp] ⚠️ desync in room '{}': {peer_addr} disagrees at tick {} (host {:016x}, client {:016x})",
                    ctx.cfg.name,
                    desync.tick,
                    desync.expected_checksum,
                    desync.client_checksum
                );
                history.record_desync(&ctx.cfg.name, 0, &desync);
            }
        } else if let Some(ref mut wasm) = room.wasm {
            if let Ok(ptr) = wasm.on_alloc(payload.len() as i32) {
                let _ = wasm.write_memory(ptr as u32, payload);
                let _ = wasm.on_input(0, ptr, payload.len() as i32);
            }
        }
        flush_room_outbox(room, ctx.transport, &ctx.cfg.name, &history);
    }

    pub(crate) fn udp_room_packet(
        &mut self,
        udp_sock: &Arc<std::net::UdpSocket>,
        peer_addr: SocketAddr,
        data: &[u8],
        ctx: &mut UdpCtx,
    ) {
        let room = match ctx.server.get_or_create(&ctx.cfg.name) {
            Ok(room) => room,
            Err(e) => {
                tracing::error!("[server-udp] ⛔ room '{}' unavailable: {e}", ctx.cfg.name);
                return;
            }
        };
        room.empty_since = None;

        let is_spec_handshake = data.len() == 5 && data[0] == 0xFF && &data[1..5] == b"SPEC";

        if !data.is_empty()
            && (data[0] == netcode::CYG_WIRE_LEAVE_ROOM || data[0] == netcode::CYG_OP_LEAVE_ROOM)
        {
            if let Some((id, sess, _)) = self.udp_peers.remove(&peer_addr) {
                tracing::info!("[server-udp] {peer_addr} left room via UDP (session {sess})");
                self.udp_input_rates.remove(&peer_addr);
                self.udp_reliable.remove(&peer_addr);
                self.udp_rxd_seq.remove(&peer_addr);
                room.sessions.remove(&sess);
                room.schema_sent.remove(&sess);
                if id > 0 {
                    room.remove_raw_player(id);
                }
            }
            return;
        }

        if !self.udp_peers.contains_key(&peer_addr) {
            if ctx.bus.is_draining() {
                tracing::info!("[server-udp] ⛔ {peer_addr} refused: server is closing");
                return;
            }
            if is_spec_handshake {
                let session_id = format!("spec_{}_{}", peer_addr.ip(), peer_addr.port());
                self.udp_peers
                    .insert(peer_addr, (0, session_id.clone(), Instant::now()));
                tracing::info!("[server-udp] 👁️  {peer_addr} connected as SPECTATOR via UDP");
                let _ = udp_sock.send_to(SPECTATOR_CONFIRM, peer_addr);
            } else {
                self.udp_join(udp_sock, peer_addr, data, ctx);
            }
        } else {
            self.udp_existing(udp_sock, peer_addr, data, is_spec_handshake, ctx);
        }
    }
}
