use crate::plane;
use crate::plane::sim::state::Plane;
use engine_spatial::aoi;

impl Plane {
    pub(super) fn emit_deltas(&mut self) {
        self.room.delta_seq = self.room.delta_seq.wrapping_add(1);
        let server_tick = self.room.current_tick as u32;

        let parsed = aoi::parse_delta(&self.delta_buf);
        if let Some((_, ref entities, _, _)) = parsed {
            for e in entities {
                // Skip the map write when unchanged — avoids dirtying the
                // entry's cache line for static entities every tick.
                if self.last_pos.get(&e.id) != Some(&e.pos) {
                    self.last_pos.insert(e.id, e.pos);
                }
            }
        }

        let sessions = &self.room.sessions;
        let delta_buf = &self.delta_buf;
        let aoi_cfg = &self.aoi_cfg;
        let tick = self.room.current_tick;
        let last_seq = &self.room.seqs;
        // Shared per-tick AOI grid: built once, queried once per viewer.
        let mut aoi_index = aoi::AoiViewIndex::new(aoi_cfg.far_radius);
        if let Some((_, entities, _, _)) = &parsed {
            aoi_index.rebuild(entities);
        }
        for (peer_id, &player_id) in sessions {
            let body: &[u8] = match &parsed {
                Some((_, entities, bullet_count, bullets_range)) => {
                    let viewer_pos = self.last_pos.get(&player_id).copied();
                    aoi::filter_for_viewer_indexed(
                        entities,
                        *bullet_count,
                        (bullets_range.0 + 4, bullets_range.1),
                        delta_buf,
                        player_id,
                        viewer_pos,
                        tick,
                        aoi_cfg,
                        &mut aoi_index,
                        &mut self.filtered_buf,
                        &mut self.visible_buf,
                    );
                    &self.filtered_buf
                }
                None => delta_buf,
            };
            let acked_seq = last_seq.get(&player_id).map(|s| s.input).unwrap_or(0);
            self.frames.clear();
            if plane::dispatch::emit_delta(
                peer_id,
                player_id,
                body,
                acked_seq,
                server_tick,
                self.room.delta_seq,
                &mut self.scratch,
                &mut self.frames,
            )
            .is_some()
            {
                if self.browser_players.contains(&player_id) {
                    for frame in self.frames.drain(..) {
                        match self.bus.deltas_tx.try_send(frame) {
                            Ok(()) => self.recorder.deltas_sent += 1,
                            Err(crossbeam_channel::TrySendError::Full(_)) => {
                                self.recorder.deltas_dropped_full += 1
                            }
                            Err(_) => break,
                        }
                    }
                } else {
                    for frame in self.frames.drain(..) {
                        if self.udp.send_frame(&frame) {
                            self.recorder.deltas_sent += 1;
                        } else {
                            self.native_send_failed += 1;
                        }
                    }
                }
            }
        }
    }
}
