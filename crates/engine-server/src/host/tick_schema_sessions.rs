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
        let sessions: Vec<(String, i64)> = room
            .sessions
            .iter()
            .map(|(s, &id)| (s.clone(), id))
            .collect();

        // Each schema section is copied ONCE per room-tick into a shared
        // allocation; every WS session's frame then references it and the
        // transport writes the same bytes for all of them (no per-client
        // body copy). Culled views and non-WS lanes keep the wrapped path.
        let full_arc: std::sync::Arc<[u8]> = schema.full.to_vec().into();
        let patch_arc: std::sync::Arc<[u8]> = schema.patch.to_vec().into();

        for (session, id) in sessions {
            let culled = match (self.schema_aoi.as_ref(), aoi_view.as_ref()) {
                (Some(layout), Some(view)) => layout.filter_culled(
                    view,
                    id as u32,
                    room.current_tick,
                    &ctx.cfg.aoi,
                    &mut self.view_buf,
                ),
                _ => false,
            };
            let is_full = force_full || !room.schema_sent.contains(&session);
            let acked_seq = room.last_input_seq.get(&session).copied().unwrap_or(0);

            if !culled {
                let body = if is_full { &full_arc } else { &patch_arc };
                if ctx
                    .transport
                    .send_delta_shared(&session, id, body.clone(), acked_seq, server_tick_u32)
                {
                    continue;
                }
            }

            let body = if culled {
                &self.view_buf[..]
            } else if is_full {
                schema.full
            } else {
                schema.patch
            };
            netcode::wrap_delta_into(&mut self.delta_body, id, body, acked_seq, server_tick_u32);
            let framed = ctx
                .transport
                .send_delta_framed(&session, delta_seq, server_tick_u32, &self.delta_body);
            if framed == 0 {
                if is_full || culled {
                    let _ = ctx.transport.send_state(&session, &self.delta_body);
                } else {
                    let _ = ctx.transport.send_to(
                        &session,
                        OpCode::RoomDataBytes,
                        &self.delta_body,
                    );
                }
            }
        }
    }
}
