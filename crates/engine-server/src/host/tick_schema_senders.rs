use engine_net::framing::OpCode;

use crate::host::*;

impl TickRunner {
    pub(crate) fn tick_schema_udp(
        &mut self,
        ctx: &mut TickCtx,
        name: &str,
        force_full: bool,
        server_tick_u32: u32,
    ) {
        let room = match ctx.server.rooms.get_mut(name) {
            Some(room) => room,
            None => return,
        };
        let mut scratch: Vec<u8> = Vec::new();
        let mut wire = udp_send::WireCount::default();
        if let Some(ref udp_sock) = ctx.udp.udp_sock {
            for (peer_addr, (peer_id, peer_session, _)) in &ctx.udp.udp_peers {
                let is_full = force_full || !room.schema_sent.contains(peer_session.as_str());
                // Patch the 10 per-peer bytes in place on the prebuilt frame.
                let frame = if is_full {
                    &mut self.wrap_full_packed
                } else {
                    &mut self.wrap_patch_packed
                };
                netcode::patch_wrap_into(
                    frame,
                    *peer_id as i64,
                    0,
                    room.seqs.get(&(*peer_id as i64)).map(|s| s.stamp).unwrap_or(0),
                );
                // One datagram per packet: a state frame past the MTU is split rather than left
                // to IP fragmentation, where one lost fragment loses the whole update.
                if udp_send::send_counted(
                    udp_sock,
                    *peer_addr,
                    frame.as_slice(),
                    server_tick_u32 as u16,
                    server_tick_u32,
                    &mut scratch,
                    &mut wire,
                )
                .is_none()
                {
                    tracing::error!(
                        "[server-udp] ⛔ frame of {} B for {peer_addr} exceeds MAX_DELTA_BODY and was dropped",
                        frame.len()
                    );
                }
                if is_full {
                    room.schema_sent
                        .insert(std::rc::Rc::from(peer_session.as_str()));
                }
            }
        }
        // Phase 0 telemetry: bytes actually written, and how often we had to split.
        ctx.udp.wire_bytes_out += wire.bytes;
        ctx.udp.wire_fragments += wire.fragmented_frames;
    }

    pub(crate) fn tick_schema_spectators(
        &mut self,
        ctx: &mut TickCtx,
        name: &str,
        force_full: bool,
        server_tick_u32: u32,
        delta_seq: u16,
    ) {
        let _ = server_tick_u32;
        let room = match ctx.server.rooms.get_mut(name) {
            Some(room) => room,
            None => return,
        };
        // Direct iteration: room borrows ctx.server, sends borrow
        // ctx.transport and self — disjoint, so no per-tick snapshot Vec.
        // (The old collect allocated a Vec<Rc<str>> per room per tick.)
        for spec in room.spectators.iter() {
            let is_full = force_full || !room.schema_sent.contains(spec.as_ref());
            // Spectator frames are identical (your_id=0, acked=0) — send the
            // prebuilt base frame directly, no per-spectator patching.
            let frame = if is_full {
                &self.wrap_full_packed
            } else {
                &self.wrap_patch_packed
            };
            let framed = ctx
                .transport
                .send_delta_framed(spec, delta_seq, server_tick_u32, frame);
            if framed == 0 {
                if is_full {
                    let _ = ctx.transport.send_state(spec, frame);
                } else {
                    let _ = ctx.transport.send_to(spec, OpCode::RoomDataBytes, frame);
                }
            }
        }
    }
}
