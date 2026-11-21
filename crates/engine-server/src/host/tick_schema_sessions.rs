use engine_net::framing::OpCode;

use crate::host::*;

/// CYG_T debug-trace gate, resolved once per process. Reading the environ on
/// every probe hit locked + linear-scanned it per session per tick.
pub(crate) fn trace_enabled() -> bool {
    static TRACE: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *TRACE.get_or_init(|| std::env::var_os("CYG_T").is_some())
}

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
        // Sections were parsed once in tick_delta — re-slice by range.
        let aoi_view = self
            .schema_aoi
            .as_ref()
            .and_then(|layout| layout.view(&self.delta_buf[self.schema_full.clone()]));
        // Reused scratch — cloning every session String per tick per room
        // was ~38k heap allocations/s at 10k CCU (the classic per-tick collect
        // the perf book warns about). Capacity survives across rooms/ticks.
        self.session_scratch.clear();
        self.session_scratch
            .extend(room.sessions.iter().map(|(s, &id)| (s.clone(), id)));
        let sessions = &self.session_scratch;

        // Shared bodies are built ONLY when the transport can actually use
        // them (WS lane + CYG_SHARED_DELTA gate). Every peer on the shared path
        // gets the same packed body; only the per-session wrap fields differ.
        let (full_arc, patch_arc) = if ctx.transport.shared_delta_enabled() {
            (
                Some(std::sync::Arc::<[u8]>::from(
                    self.packed_full_body.as_slice(),
                )),
                Some(std::sync::Arc::<[u8]>::from(
                    self.packed_patch_body.as_slice(),
                )),
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
            let stamp = room.seqs.get(id).map(|s| s.stamp).unwrap_or(0);

            if !culled {
                let shared = match (is_full, &full_arc, &patch_arc) {
                    (true, Some(a), _) => Some(a.clone()),
                    (false, _, Some(a)) => Some(a.clone()),
                    _ => None,
                };
                if let Some(body) = shared
                    && ctx.transport.send_delta_shared(
                        session,
                        *id,
                        body,
                        acked_seq,
                        server_tick_u32,
                        stamp,
                    )
                {
                    continue;
                }
            }

            if culled {
                // Culled views are per-session. The view buffer is still the
                // fixed-width guest layout (the host built it from the guest's
                // own section); it is transcoded to the packed wire here.
                // Inline (not a `&mut self` helper) so the AOI view borrow of
                // `delta_buf` stays live: the packed lane and scratch buffers
                // are disjoint fields.
                let packed = match self.packed_lane.as_mut() {
                    Some(lane) => match lane.full(&self.view_buf) {
                        Ok(bytes) => bytes,
                        Err(error) => {
                            tracing::error!(
                                "[host] packed AOI view transcode failed for `{session}`: {error} — session skipped this tick"
                            );
                            continue;
                        }
                    },
                    None => {
                        tracing::error!(
                            "[host] room `{name}` has state but no packed transcoder — `{session}` skipped"
                        );
                        continue;
                    }
                };
                self.packed_view_buf = packed;
                netcode::wrap_delta_into(
                    &mut self.delta_body,
                    *id,
                    &self.packed_view_buf,
                    acked_seq,
                    server_tick_u32,
                    stamp,
                );
            } else {
                let base = if is_full {
                    &mut self.wrap_full_packed
                } else {
                    &mut self.wrap_patch_packed
                };
                netcode::patch_wrap_into(base, *id, acked_seq, stamp);
            }

            let frame: &[u8] = if culled {
                &self.delta_body
            } else if is_full {
                &self.wrap_full_packed
            } else {
                &self.wrap_patch_packed
            };
            let framed = ctx.transport.send_delta_framed(session, delta_seq, server_tick_u32, frame);
            if trace_enabled() && is_full {
                eprintln!("[t] FULL{session} framed={framed} culled={culled}");
            }
            if framed == 0 {
                if is_full || culled {
                    let _ = ctx.transport.send_state(session, frame);
                } else {
                    let _ = ctx.transport.send_to(session, OpCode::RoomDataBytes, frame);
                }
            }
        }
    }
}
