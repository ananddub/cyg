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
        let (full_arc, patch_arc) = if !self.private_state && ctx.transport.shared_delta_enabled() {
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
            let view_changed = room.view_dirty.contains(id);
            let is_full = force_full || view_changed || !room.schema_sent.contains(session);
            let view = room.view_masks.get(id);
            let per_view = self.private_state || view.is_some();
            let acked_seq = room.seqs.get(id).map(|s| s.input).unwrap_or(0);
            let stamp = room.seqs.get(id).map(|s| s.stamp).unwrap_or(0);

            if !culled && !per_view && !self.volatile_ready {
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

            if culled || per_view {
                // Per-session view: AOI already removed entities from the
                // packed section, `.private()` fields are blanked for every row
                // that is not this viewer's own, and the guest's view mask
                // removes any extra fields/rows it asked to hide.
                let packed = if culled {
                    match (self.packed_lane.as_mut(), view) {
                        (Some(lane), Some(view)) => {
                            lane.clear_deferred(name, *id as u32);
                            lane.full_for_view(&self.view_buf, *id as u32, view)
                        }
                        (Some(lane), None) => {
                            lane.clear_deferred(name, *id as u32);
                            lane.full_for(&self.view_buf, *id as u32)
                        }
                        (None, _) => {
                            tracing::error!(
                                "[host] room `{name}` has state but no packed schema — `{session}` skipped"
                            );
                            continue;
                        }
                    }
                } else {
                    let section = if is_full {
                        &self.delta_buf[self.schema_full.clone()]
                    } else {
                        &self.delta_buf[self.schema_patch.clone()]
                    };
                    match (self.packed_lane.as_mut(), view) {
                        (Some(lane), Some(view)) if is_full => {
                            lane.clear_deferred(name, *id as u32);
                            lane.full_for_view(section, *id as u32, view)
                        }
                        (Some(lane), Some(view)) => lane.patch_for_view_budget(
                            name,
                            section,
                            *id as u32,
                            view,
                            crate::netcode::packed_lane::DEFAULT_PATCH_BUDGET,
                        ),
                        (Some(lane), None) if is_full => {
                            lane.clear_deferred(name, *id as u32);
                            lane.full_for(section, *id as u32)
                        }
                        (Some(lane), None) => lane.patch_for_budget(
                            name,
                            section,
                            *id as u32,
                            crate::netcode::packed_lane::DEFAULT_PATCH_BUDGET,
                        ),
                        (None, _) => {
                            tracing::error!(
                                "[host] room `{name}` has state but no packed schema — `{session}` skipped"
                            );
                            continue;
                        }
                    }
                };
                let packed = match packed {
                    Ok(bytes) => bytes,
                    Err(error) => {
                        tracing::error!(
                            "[host] packed view processing failed for `{session}`: {error} — session skipped this tick"
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

            let frame: &[u8] = if culled || per_view {
                &self.delta_body
            } else if is_full {
                &self.wrap_full_packed
            } else {
                &self.wrap_patch_packed
            };
            let mut delivered = if self.volatile_ready {
                // With volatile fields present the must-deliver stream takes
                // the reliable lane; only the marked fields ride the lossy one.
                ctx.transport.send_state_reliable(session, frame, server_tick_u32).is_ok()
            } else {
                let framed =
                    ctx.transport
                        .send_delta_framed(session, delta_seq, server_tick_u32, frame);
                let mut delivered = framed > 0;
                if framed == 0 {
                    let fallback = if is_full || culled {
                        ctx.transport.send_state(session, frame)
                    } else {
                        ctx.transport.send_to(session, OpCode::RoomDataBytes, frame)
                    };
                    delivered = fallback.is_ok();
                }
                delivered
            };
            if let Some(volatile_body) = super::tick_delta::volatile_body_for(
                &mut self.packed_lane,
                self.volatile_ready,
                self.volatile_per_view,
                &self.packed_volatile_body,
                *id as u32,
                view,
            ) {
                let mut wrapped = Vec::with_capacity(volatile_body.len() + 32);
                netcode::wrap_delta_into(
                    &mut wrapped,
                    *id,
                    &volatile_body,
                    acked_seq,
                    server_tick_u32,
                    stamp,
                );
                let mut volatile = Vec::with_capacity(wrapped.len() + 5);
                volatile.push(0xA6);
                volatile.extend_from_slice(&server_tick_u32.to_le_bytes());
                volatile.extend_from_slice(&wrapped);
                let _ = ctx.transport.send_volatile(session, &volatile);
            }
            if view_changed && delivered {
                // The full went out on this tick. A later patch can safely
                // overwrite the client's copy from now on.
                room.view_dirty.remove(id);
            }
            if trace_enabled() && is_full {
                eprintln!(
                    "[t] FULL{session} delivered={delivered} culled={culled} private={} view={}",
                    self.private_state,
                    view.is_some()
                );
            }
        }
    }
}
