use engine_net::framing::OpCode;

use crate::host::*;

impl TickRunner {
    pub(crate) fn tick_legacy_players(
        &mut self,
        ctx: &mut TickCtx,
        name: &str,
        server_tick_u32: u32,
        delta_seq: u16,
    ) {
        let room = match ctx.server.rooms.get_mut(name) {
            Some(room) => room,
            None => return,
        };
        let delta = &self.delta_buf[..];
        let aoi_sections = if ctx.cfg.aoi.enabled {
            netcode::parse_delta_sections(delta)
        } else {
            None
        };
        for (session, id) in &room.sessions {
            if let Some((_, entities, bullet_count, bc_off, bullet_end)) = &aoi_sections {
                let viewer_pos = entities.iter().find(|e| e.id == *id).map(|e| e.pos);
                netcode::filter_delta_for_viewer(
                    entities,
                    *bullet_count,
                    (*bc_off + 4, *bullet_end),
                    delta,
                    *id,
                    viewer_pos,
                    room.current_tick,
                    &ctx.cfg.aoi,
                    &mut self.filtered_buf,
                );
                let acked_seq = room.last_input_seq.get(session).copied().unwrap_or(0);
                netcode::wrap_delta_into(
                    &mut self.delta_body,
                    *id,
                    &self.filtered_buf,
                    acked_seq,
                    server_tick_u32,
                );
            } else {
                let acked_seq = room.last_input_seq.get(session).copied().unwrap_or(0);
                netcode::wrap_delta_into(
                    &mut self.delta_body,
                    *id,
                    delta,
                    acked_seq,
                    server_tick_u32,
                );
            }
            let framed = ctx
                .transport
                .send_delta_framed(session, delta_seq, server_tick_u32, &self.delta_body);
            if framed == 0 {
                let _ = ctx
                    .transport
                    .send_to(session, OpCode::RoomDataBytes, &self.delta_body);
            }
        }
    }

    pub(crate) fn tick_legacy_spectators(
        &mut self,
        ctx: &mut TickCtx,
        name: &str,
        server_tick_u32: u32,
        delta_seq: u16,
    ) {
        let room = match ctx.server.rooms.get_mut(name) {
            Some(room) => room,
            None => return,
        };
        let delta = &self.delta_buf[..];
        if room.spectators.is_empty() {
            return;
        }
        netcode::wrap_delta_into(&mut self.delta_body, 0, delta, 0, server_tick_u32);
        for spec_session in &room.spectators {
            let framed = ctx.transport.send_delta_framed(
                spec_session,
                delta_seq,
                server_tick_u32,
                &self.delta_body,
            );
            if framed == 0 {
                let _ = ctx
                    .transport
                    .send_to(spec_session, OpCode::RoomDataBytes, &self.delta_body);
            }
        }
    }
}
