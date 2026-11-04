use std::time::Instant;

use crate::host::*;

impl TickRunner {
    pub(crate) fn tick_delta(&mut self, ctx: &mut TickCtx, name: &str) {
        if netcode::split_schema(&self.delta_buf[..]).is_some() {
            self.tick_schema(ctx, name);
        } else {
            self.tick_legacy(ctx, name);
        }
    }

    pub(crate) fn tick_schema(&mut self, ctx: &mut TickCtx, name: &str) {
        let room = match ctx.server.rooms.get_mut(name) {
            Some(room) => room,
            None => return,
        };
        let delta = &self.delta_buf[..];
        let Some(schema) = netcode::split_schema(delta) else {
            return;
        };
        let force_full = if room.patch_due_wired {
            // The guest gated its own production: a delivered delta already
            // contains every change since the last send, so send it as-is.
            room.mark_patch_sent(Instant::now());
            false
        } else if !room.should_send_patch(Instant::now(), ctx.cfg.patch_rate_ms()) {
            // Legacy guest (no on_patch_due export): skip the send and force a
            // full snapshot next time, because the guest cleared its dirty
            // rows at commit and the skipped diffs are unrecoverable.
            return;
        } else {
            room.take_patch_skipped()
        };
        if force_full {
            room.schema_sent.clear();
        }
        let server_tick_u32 = room.current_tick as u32;
        room.delta_seq = room.delta_seq.wrapping_add(1);
        let delta_seq = room.delta_seq;
        let aoi_view = self
            .schema_aoi
            .as_ref()
            .and_then(|layout| layout.view(schema.full));
        let quiet = schema.patch.is_empty()
            && !force_full
            && room
                .sessions
                .keys()
                .chain(room.spectators.iter())
                .all(|s| room.schema_sent.contains(s))
            && aoi_view.is_none();
        if !quiet {
            self.tick_schema_sessions(ctx, name, force_full, server_tick_u32, delta_seq);
            self.tick_schema_udp(ctx, name, force_full, server_tick_u32);
            self.tick_schema_spectators(ctx, name, force_full, server_tick_u32, delta_seq);
        }
    }
}
