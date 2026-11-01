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
        let room = match ctx.server.get_or_create(&ctx.cfg.name) {
            Ok(room) => room,
            Err(_) => return,
        };
        let n = room.sessions.len() + self.udp_peers.len();
        let (x, y) = (
            60.0 + ((n as f64) * 97.0) % (ctx.cfg.width - 120.0),
            60.0 + ((n as f64) * 61.0) % (ctx.cfg.height - 120.0),
        );
        let id = room.add_raw_player(x, y);
        let session_id = format!("p{}_{}_{}", id, peer_addr.ip(), peer_addr.port());
        self.udp_peers
            .insert(peer_addr, (id, session_id.clone(), Instant::now()));
        self.udp_reliable.entry(peer_addr).or_default();
        tracing::info!(
            "[server-udp] {peer_addr} joined room via UDP (player id {id}, session {session_id})"
        );

        let rt = format!("{}:{}", ctx.cfg.name, session_id);
        let mut join_reply = Vec::with_capacity(1 + 1 + rt.len() + 1 + 6 + 1);
        join_reply.push(netcode::CYG_WIRE_JOIN_ROOM);
        join_reply.push(rt.len() as u8);
        join_reply.extend_from_slice(rt.as_bytes());
        join_reply.push(6);
        join_reply.extend_from_slice(b"schema");
        join_reply.push(0x00);
        let _ = udp_sock.send_to(&join_reply, peer_addr);
        if !self.schema_descriptor.is_empty() {
            let _ = udp_sock.send_to(&self.schema_descriptor, peer_addr);
        }

        if data.len() == 20 {
            let Ok(sample) = <&[u8; 20]>::try_from(data) else {
                return;
            };
            if !udp_input_allowed(
                &mut self.udp_input_rates,
                ctx.tick_interval,
                peer_addr,
                sample,
            ) {
                ctx.recorder.inputs_dropped += 1;
                return;
            }
            if let Some(ref mut wasm) = room.wasm {
                if let Ok(ptr) = wasm.on_alloc(sample.len() as i32) {
                    let _ = wasm.write_memory(ptr as u32, sample);
                    let _ = wasm.on_input(id, ptr, sample.len() as i32);
                }
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
        let room = match ctx.server.get_or_create(&ctx.cfg.name) {
            Ok(room) => room,
            Err(_) => return,
        };
        room.empty_since = None;
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
                    let _sample = match <&[u8; 20]>::try_from(sample_arr) {
                        Ok(s) => s,
                        Err(_) => return,
                    };
                    let Ok(sample) = <&[u8; 20]>::try_from(data) else {
                        return;
                    };
                    if !udp_input_allowed(&mut self.udp_input_rates, ctx.tick_interval, peer_addr, sample)
                    {
                        ctx.recorder.inputs_dropped += 1;
                        return;
                    }
                    if let Some(ref mut wasm) = room.wasm {
                        if let Ok(ptr) = wasm.on_alloc(data.len() as i32) {
                            let _ = wasm.write_memory(ptr as u32, data);
                            let _ = wasm.on_input(*id, ptr, data.len() as i32);
                        }
                    }
                }
            }
        }
    }
}
