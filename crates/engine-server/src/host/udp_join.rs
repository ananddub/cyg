use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Instant;

use crate::host::*;

impl UdpLane {
    pub(crate) fn udp_join(
        &mut self,
        udp_sock: &Arc<std::net::UdpSocket>,
        peer_addr: SocketAddr,
        data: &[u8],
        ctx: &mut UdpCtx,
    ) {
        if self.udp_join_rates.len() >= 4096 && !self.udp_join_rates.contains_key(&peer_addr) {
            return;
        }
        if !self
            .udp_join_rates
            .entry(peer_addr)
            .or_insert_with(|| netcode::InputRateLimiter::new(5, 5.0))
            .allow(Instant::now())
        {
            tracing::error!("[server-udp] ⛔ join rate-limit hit by {peer_addr}");
            return;
        }
        // The join envelope is the same one the pinned sim plane accepts:
        // `[0x20][token_len][seat_token]`. A raw peer without a minted,
        // unexpired seat is not admitted, even if it sends a valid-looking
        // 20-byte input frame first.
        const OP_DENY: u8 = 0x25;

        let admitted = match crate::netcode::admission::admit_join_frame(
            crate::netcode::admission::Lane::UdpHost,
            &self.seats,
            data,
            None,
        ) {
            Ok(a) => a,
            Err(reason) => {
                tracing::error!(
                    "[server-udp] ⛔ {peer_addr} rejected by admission: {reason:?}"
                );
                let _ = udp_sock.send_to(&[OP_DENY, reason.code()], peer_addr);
                return;
            }
        };
        let room_name = admitted.room;

        if ctx
            .server
            .would_exceed_room_limit(&room_name, ctx.cfg.max_rooms)
        {
            tracing::error!(
                "[server-udp] ⛔ room limit reached: refusing UDP join from {peer_addr}"
            );
            let _ = udp_sock.send_to(&[OP_DENY, crate::netcode::admission::DenyReason::RoomFull.code()], peer_addr);
            return;
        }
        let room = match ctx.server.get_or_create(&room_name, b"") {
            Ok(room) => room,
            Err(_) => return,
        };
        if room.is_full(ctx.cfg.max_clients) {
            tracing::error!(
                "[server-udp] ⛔ room '{room_name}' is full ({}/{}): refusing UDP join from {peer_addr}",
                room.sessions.len(),
                ctx.cfg.max_clients
            );
            let _ = udp_sock.send_to(&[OP_DENY, crate::netcode::admission::DenyReason::RoomFull.code()], peer_addr);
            return;
        }
        // Same onAuth gate the TCP/WS path applies; UDP joins carry no options.
        if !room.authorize(room.next_player_id, "", "{}") {
            tracing::error!("[server-udp] ⛔ room '{room_name}' rejected {peer_addr} in onAuth");
            return;
        }
        let n = room.sessions.len() + self.peers_in_room(&room_name);
        let (x, y) = (
            60.0 + ((n as f64) * 97.0) % (ctx.cfg.width - 120.0),
            60.0 + ((n as f64) * 61.0) % (ctx.cfg.height - 120.0),
        );
        let id = room.add_raw_player(x, y);
        let session_id = format!("p{}_{}_{}", id, peer_addr.ip(), peer_addr.port());
        self.udp_peers
            .insert(peer_addr, (id, session_id.clone(), Instant::now()));
        self.udp_rooms.insert(peer_addr, room_name.clone());
        self.udp_reliable.entry(peer_addr).or_default();
        tracing::info!(
            "[server-udp] {peer_addr} joined room '{room_name}' via UDP (player id {id}, session {session_id})"
        );

        // The current raw-UDP bind, identical to the pinned plane's: player id,
        // tick rate, and room name. The legacy room:session string is gone, so
        // one client decoder serves every raw-UDP lane.
        let mut bind = Vec::new();
        netcode::encode_bind(id, ctx.cfg.tick_rate as f64, &room_name, &mut bind);
        let _ = udp_sock.send_to(&bind, peer_addr);
        if !self.schema_descriptor.is_empty() {
            let descriptor = self.schema_descriptor.clone();
            if descriptor.len() <= 1200
                || !self.start_chunk(peer_addr, netcode::CYG_WIRE_SCHEMA, &descriptor)
            {
                let _ = udp_sock.send_to(&descriptor, peer_addr);
            }
        }
    }

    pub(crate) fn udp_existing(
        &mut self,
        udp_sock: &Arc<std::net::UdpSocket>,
        peer_addr: SocketAddr,
        data: &[u8],
        is_spec_handshake: bool,
        ctx: &mut UdpCtx,
    ) {
        let Some(room_name) = self.peer_room(peer_addr).map(str::to_owned) else {
            tracing::error!("[server-udp] ⛔ {peer_addr} has no admitted room; dropping packet");
            return;
        };
        let room = match ctx.server.get_or_create(&room_name, b"") {
            Ok(room) => room,
            Err(_) => return,
        };
        room.lifecycle.empty_since = None;
        if let Some((id, _peer_sess, last_seen)) = self.udp_peers.get_mut(&peer_addr) {
            *last_seen = Instant::now();
            if is_spec_handshake && *id != 0 {
                room.remove_raw_player(*id);
                *id = 0;
                tracing::info!("[server-udp] 👁️  {peer_addr} upgraded to SPECTATOR via UDP");
            } else if *id > 0 && !data.is_empty() {
                if data[0] == netcode::CYG_WIRE_PING {
                    let _ = udp_sock.send_to(data, peer_addr);
                    return;
                }
                let input_sample: Option<&[u8]> = if data.len() == 20 {
                    Some(data)
                } else if data.len() == 23 && data[0] == 0x21 {
                    Some(&data[3..23])
                } else {
                    None
                };
                if let Some(sample_arr) = input_sample {
                    // The 23-byte frame carries the 20-byte sample at offset 3;
                    // the payload handed to the guest is always the sample.
                    let Ok(mut sample) = <[u8; 20]>::try_from(sample_arr) else {
                        return;
                    };
                    if !udp_input_allowed(
                        &mut self.udp_input_rates,
                        ctx.tick_interval,
                        peer_addr,
                        &mut sample,
                        &room.input_domains,
                    ) {
                        ctx.recorder.inputs_dropped += 1;
                        return;
                    }
                    if let Some(ref mut wasm) = room.wasm {
                        if let Ok(ptr) = wasm.on_alloc(sample.len() as i32) {
                            let _ = wasm.write_memory(ptr as u32, &sample);
                            let _ = wasm.on_input(*id, ptr, sample.len() as i32);
                        }
                    }
                }
            }
        }
    }
}
