use crate::host::*;

impl TickRunner {
    pub(crate) fn tick_legacy(&mut self, ctx: &mut TickCtx, name: &str) {
        let room = match ctx.server.rooms.get_mut(name) {
            Some(room) => room,
            None => return,
        };
        let delta = &self.delta_buf[..];
        let server_tick_u32 = room.current_tick as u32;
        room.delta_seq = room.delta_seq.wrapping_add(1);
        let delta_seq = room.delta_seq;
        let aoi_sections = if ctx.cfg.aoi.enabled {
            netcode::parse_delta_sections(delta)
        } else {
            None
        };

        if let Some((_, ref entities, _, _, _)) = aoi_sections {
            for e in entities {
                room.voice_room.update_position(e.id as u32, e.pos);
            }
        } else if delta.len() >= 4 {
            let count = u32::from_le_bytes(delta[0..4].try_into().unwrap_or([0; 4])) as usize;
            for i in 0..count {
                let base = 4 + i * 72;
                if base + 32 <= delta.len() {
                    let pid = i64::from_le_bytes(
                        delta[base..base + 8].try_into().unwrap_or([0; 8]),
                    );
                    let px = f64::from_le_bytes(
                        delta[base + 8..base + 16].try_into().unwrap_or([0; 8]),
                    );
                    let py = f64::from_le_bytes(
                        delta[base + 16..base + 24].try_into().unwrap_or([0; 8]),
                    );
                    let pz = f64::from_le_bytes(
                        delta[base + 24..base + 32].try_into().unwrap_or([0; 8]),
                    );
                    room.voice_room.update_position(pid as u32, [px, py, pz]);
                }
            }
        }

        self.tick_legacy_players(ctx, name, server_tick_u32, delta_seq);
        self.tick_legacy_spectators(ctx, name, server_tick_u32, delta_seq);
        self.tick_legacy_udp(ctx, name, server_tick_u32);
    }
}
