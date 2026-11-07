use engine_net::framing::OpCode;

use crate::host::*;

impl TickRunner {
    pub(crate) fn tick_schema_sessions(
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
        let delta = &self.delta_buf[..];
        let Some(schema) = netcode::split_schema(delta) else {
            return;
        };
        let aoi_view = self
            .schema_aoi
            .as_ref()
            .and_then(|layout| layout.view(schema.full));
        // Reused scratch — cloning every session String per tick per room
        // was ~38k heap allocations/s at 10k CCU (the classic per-tick collect
        // the perf book warns about). Capacity survives across rooms/ticks.
        self.session_scratch.clear();
        self.session_scratch
            .extend(room.sessions.iter().map(|(s, &id)| (s.clone(), id)));
        let sessions = &self.session_scratch;

        // Shared bodies are built ONLY when the transport can actually use
        // them (WS lane + CYG_SHARED_DELTA gate). Building them unconditionally
        // allocated 2 Arcs + 2 copies per room-tick that the legacy path threw
        // straight away.
        let (full_arc, patch_arc) = if ctx.transport.shared_delta_enabled() {
            (
                Some(std::sync::Arc::<[u8]>::from(schema.full.to_vec())),
                Some(std::sync::Arc::<[u8]>::from(schema.patch.to_vec())),
            )
        } else {
            (None, None)
        };

        for (session, id) in sessions.iter() {
            let culled = match (self.schema_aoi.as_ref(), aoi_view.as_ref()) {
                (Some(layout), Some(view)) => layout.filter_culled(
                    view,
                    *id as u32,
                    room.current_tick,
                    &ctx.cfg.aoi,
                    &mut self.view_buf,
                ),
                _ => false,
            };
            let is_full = force_full || !room.schema_sent.contains(session);
            let acked_seq = room.seqs.get(id).map(|s| s.input).unwrap_or(0);

            if !culled {
                let shared = match (is_full, &full_arc, &patch_arc) {
                    (true, Some(a), _) => Some(a.clone()),
                    (false, _, Some(a)) => Some(a.clone()),
                    _ => None,
                };
                if let Some(body) = shared {
                    if ctx
                        .transport
                        .send_delta_shared(session, *id, body, acked_seq, server_tick_u32)
                    {
                        continue;
                    }
                }
            }

            let body = if culled {
                &self.view_buf[..]
            } else if is_full {
                schema.full
            } else {
                schema.patch
            };
            netcode::wrap_delta_into(&mut self.delta_body, *id, body, acked_seq, server_tick_u32);
            let framed = ctx
                .transport
                .send_delta_framed(session, delta_seq, server_tick_u32, &self.delta_body);
            if framed == 0 {
                if is_full || culled {
                    let _ = ctx.transport.send_state(session, &self.delta_body);
                } else {
                    let _ =
                        ctx.transport.send_to(session, OpCode::RoomDataBytes, &self.delta_body);
                }
            }
        }
    }
}
