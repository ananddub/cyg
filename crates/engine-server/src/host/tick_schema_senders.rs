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
                let view_changed = room.view_dirty.contains(&(*peer_id as i64));
                let is_full = force_full
                    || view_changed
                    || !room.schema_sent.contains(peer_session.as_str());
                let stamp = room
                    .seqs
                    .get(&(*peer_id as i64))
                    .map(|s| s.stamp)
                    .unwrap_or(0);
                let view = room.view_masks.get(&(*peer_id as i64));
                let per_view = self.private_state || view.is_some();
                let budget = ctx
                    .udp
                    .udp_reliable
                    .get(peer_addr)
                    .map(|lane| {
                        crate::netcode::packed_lane::adaptive_patch_budget(
                            lane.pacing_bps(),
                            ctx.tick_interval,
                        )
                    })
                    .unwrap_or(crate::netcode::packed_lane::DEFAULT_PATCH_BUDGET);
                // Patch the 10 per-peer bytes in place on the prebuilt frame.
                let frame = if per_view {
                    let section = if is_full {
                        &self.delta_buf[self.schema_full.clone()]
                    } else {
                        &self.delta_buf[self.schema_patch.clone()]
                    };
                    let packed = match (self.packed_lane.as_mut(), view) {
                        (Some(lane), Some(view)) if is_full => {
                            lane.clear_deferred(name, *peer_id as u32);
                            lane.full_for_view(section, *peer_id as u32, view)
                        }
                        (Some(lane), Some(view)) => lane.patch_for_view_budget(
                            name,
                            section,
                            *peer_id as u32,
                            view,
                            budget,
                        ),
                        (Some(lane), None) if is_full => {
                            lane.clear_deferred(name, *peer_id as u32);
                            lane.full_for(section, *peer_id as u32)
                        }
                        (Some(lane), None) => lane.patch_for_budget(
                            name,
                            section,
                            *peer_id as u32,
                            crate::netcode::packed_lane::DEFAULT_PATCH_BUDGET,
                        ),
                        (None, _) => continue,
                    };
                    let packed = match packed {
                        Ok(bytes) => bytes,
                        Err(error) => {
                            tracing::error!("[host] UDP packed view failed: {error}");
                            continue;
                        }
                    };
                    self.packed_view_buf = packed;
                    netcode::wrap_delta_into(
                        &mut self.delta_body,
                        *peer_id,
                        &self.packed_view_buf,
                        0,
                        server_tick_u32,
                        stamp,
                    );
                    &self.delta_body
                } else {
                    let frame = if is_full {
                        &mut self.wrap_full_packed
                    } else {
                        &mut self.wrap_patch_packed
                    };
                    netcode::patch_wrap_into(frame, *peer_id as i64, 0, stamp);
                    frame
                };
                // One datagram per packet: a state frame past the MTU is split rather than left
                // to IP fragmentation, where one lost fragment loses the whole update.
                let delivered = udp_send::send_counted(
                    udp_sock,
                    *peer_addr,
                    frame.as_slice(),
                    server_tick_u32 as u16,
                    server_tick_u32,
                    &mut scratch,
                    &mut wire,
                );
                if delivered.is_none() {
                    tracing::error!(
                        "[server-udp] ⛔ frame of {} B for {peer_addr} exceeds MAX_DELTA_BODY and was dropped",
                        frame.len()
                    );
                }
                if let Some(volatile_body) = super::tick_delta::volatile_body_for(
                    &mut self.packed_lane,
                    self.volatile_ready,
                    self.volatile_per_view,
                    &self.packed_volatile_body,
                    *peer_id as u32,
                    view,
                ) {
                    let mut wrapped = Vec::with_capacity(volatile_body.len() + 32);
                    netcode::wrap_delta_into(
                        &mut wrapped,
                        *peer_id,
                        &volatile_body,
                        0,
                        server_tick_u32,
                        stamp,
                    );
                    let mut volatile = Vec::with_capacity(wrapped.len() + 5);
                    volatile.push(0xA6);
                    volatile.extend_from_slice(&server_tick_u32.to_le_bytes());
                    volatile.extend_from_slice(&wrapped);
                    let _ = udp_send::send_counted(
                        udp_sock,
                        *peer_addr,
                        &volatile,
                        server_tick_u32 as u16,
                        server_tick_u32,
                        &mut scratch,
                        &mut wire,
                    );
                }
                if is_full {
                    room.schema_sent
                        .insert(std::rc::Rc::from(peer_session.as_str()));
                }
                if view_changed && delivered.is_some() {
                    room.view_dirty.remove(&(*peer_id as i64));
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
        let room = match ctx.server.rooms.get_mut(name) {
            Some(room) => room,
            None => return,
        };
        // Direct iteration: room borrows ctx.server, sends borrow
        // ctx.transport and self — disjoint, so no per-tick snapshot Vec.
        // (The old collect allocated a Vec<Rc<str>> per room per tick.)
        for spec in room.spectators.iter() {
            let is_full = force_full || !room.schema_sent.contains(spec.as_ref());
            let frame: &[u8] = if self.private_state {
                // Spectators are not a row owner, so every `.private()` field
                // is blanked. The body is identical for all spectators, but
                // this path is rare enough to keep the code obvious.
                let section = if is_full {
                    &self.delta_buf[self.schema_full.clone()]
                } else {
                    &self.delta_buf[self.schema_patch.clone()]
                };
                let packed = match self.packed_lane.as_mut() {
                    Some(lane) if is_full => {
                        lane.clear_deferred(name, u32::MAX);
                        lane.full_for(section, u32::MAX)
                    }
                    Some(lane) => lane.patch_for_budget(
                        name,
                        section,
                        u32::MAX,
                        crate::netcode::packed_lane::DEFAULT_PATCH_BUDGET,
                    ),
                    None => continue,
                };
                let packed = match packed {
                    Ok(bytes) => bytes,
                    Err(error) => {
                        tracing::error!("[host] spectator packed view failed: {error}");
                        continue;
                    }
                };
                self.packed_view_buf = packed;
                netcode::wrap_delta_into(
                    &mut self.delta_body,
                    0,
                    &self.packed_view_buf,
                    0,
                    server_tick_u32,
                    0,
                );
                &self.delta_body
            } else if is_full {
                &self.wrap_full_packed
            } else {
                &self.wrap_patch_packed
            };
            if let Some(volatile_body) = super::tick_delta::volatile_body_for(
                &mut self.packed_lane,
                self.volatile_ready,
                self.volatile_per_view,
                &self.packed_volatile_body,
                u32::MAX,
                None,
            ) {
                let _ = ctx.transport.send_state_reliable(spec, frame, server_tick_u32);
                let mut wrapped = Vec::with_capacity(volatile_body.len() + 32);
                netcode::wrap_delta_into(
                    &mut wrapped,
                    0,
                    &volatile_body,
                    0,
                    server_tick_u32,
                    0,
                );
                let mut volatile = Vec::with_capacity(wrapped.len() + 5);
                volatile.push(0xA6);
                volatile.extend_from_slice(&server_tick_u32.to_le_bytes());
                volatile.extend_from_slice(&wrapped);
                let _ = ctx.transport.send_volatile(spec, &volatile);
            } else {
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
}
