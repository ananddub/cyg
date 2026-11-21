use std::time::Instant;

use crate::host::*;

impl TickRunner {
    /// Transcode the tick's guest full/patch sections into the packed wire.
    /// This is the only state wire format: the result is shared by every peer,
    /// and only `your_id` and the ack tail are patched per session. There is no
    /// fixed-width fallback — a transcode failure is a bug and is logged, and
    /// this tick's state is dropped rather than sent in an older format.
    fn build_packed(&mut self, server_tick_u32: u32) {
        self.packed_ready = false;
        let Some(lane) = self.packed_lane.as_mut() else {
            return;
        };
        let full = lane.full(&self.delta_buf[self.schema_full.clone()]);
        let patch = lane.patch(&self.delta_buf[self.schema_patch.clone()]);
        match (full, patch) {
            (Ok(full), Ok(patch)) => {
                self.packed_full_body = full;
                self.packed_patch_body = patch;
                netcode::wrap_delta_into(
                    &mut self.wrap_full_packed,
                    0,
                    &self.packed_full_body,
                    0,
                    server_tick_u32,
                    0,
                );
                netcode::wrap_delta_into(
                    &mut self.wrap_patch_packed,
                    0,
                    &self.packed_patch_body,
                    0,
                    server_tick_u32,
                    0,
                );
                self.packed_ready = true;
            }
            (full, patch) => {
                if let Err(error) = full {
                    tracing::error!("[host] packed full transcode failed: {error}");
                }
                if let Err(error) = patch {
                    tracing::error!("[host] packed patch transcode failed: {error}");
                }
            }
        }
    }

    pub(crate) fn tick_delta(&mut self, ctx: &mut TickCtx, name: &str) {
        // Parse ONCE here and remember the section ranges. Every downstream
        // sender re-slices `delta_buf` from these instead of re-parsing — the
        // same buffer was being split up to 5× per room-tick before.
        let Some(sections) = netcode::split_schema(&self.delta_buf[..]) else {
            // The legacy (schema-less) delta path is gone; a guest that still
            // emits it is an old build and gets its deltas dropped until it
            // recompiles against the current SDK.
            tracing::warn!("[host] room `{name}` produced a non-schema delta — skipped (rebuild the room with the current SDK)");
            return;
        };
        // Slices are into `delta_buf` — record their byte ranges.
        let base = self.delta_buf.as_ptr() as usize;
        self.schema_full = (sections.full.as_ptr() as usize - base)
            ..(sections.full.as_ptr() as usize - base + sections.full.len());
        self.schema_patch = (sections.patch.as_ptr() as usize - base)
            ..(sections.patch.as_ptr() as usize - base + sections.patch.len());
        self.tick_schema(ctx, name);
    }

    pub(crate) fn tick_schema(&mut self, ctx: &mut TickCtx, name: &str) {
        let room = match ctx.server.rooms.get_mut(name) {
            Some(room) => room,
            None => return,
        };
        let (full, patch) = (&self.delta_buf[self.schema_full.clone()], &self.delta_buf[self.schema_patch.clone()]);
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
            .and_then(|layout| layout.view(full));
        let quiet = patch.is_empty()
            && !force_full
            && room
                .sessions
                .keys()
                .chain(room.spectators.iter())
                .all(|s| room.schema_sent.contains(s))
            && aoi_view.is_none();
        if quiet {
            return;
        }
        // Prebuild the two wrapped packed frames ONCE per room-tick
        // (your_id=0, acked=0, stamp=0). Session sends patch the 10 varying
        // bytes in place — the old path re-wrapped the whole body per session.
        self.build_packed(server_tick_u32);
        if !self.packed_ready {
            tracing::error!(
                "[host] room `{name}` produced state that the packed lane could not encode — tick dropped"
            );
            return;
        }
        self.tick_schema_sessions(ctx, name, force_full, server_tick_u32, delta_seq);
        self.tick_schema_udp(ctx, name, force_full, server_tick_u32);
        self.tick_schema_spectators(ctx, name, force_full, server_tick_u32, delta_seq);
    }
}
