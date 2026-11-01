use std::time::{Duration, Instant};

use crate::plane::sim::helpers::decode_packed;
use crate::plane::sim::state::Plane;

impl Plane {
    pub(super) fn run_ticks(&mut self, elapsed: Duration) -> u32 {
        let period = self.period;
        let watchdog = self.cfg.watchdog.clone();
        let mut ticks_to_run = (elapsed.as_nanos() / period.as_nanos().max(1) as u128) as u32;
        if ticks_to_run > watchdog.max_ticks_per_frame {
            self.recorder.tick_overruns += (ticks_to_run - watchdog.max_ticks_per_frame) as u64;
            ticks_to_run = watchdog.max_ticks_per_frame;
            self.last_tick = Instant::now();
        } else {
            self.last_tick += period * ticks_to_run;
        }
        let t0 = Instant::now();
        self.tick_t0 = t0;

        let dt = self.dt;
        let mut trap = false;
        for _ in 0..ticks_to_run {
            self.room.current_tick += 1;
            if let Some(ref mut wasm) = self.room.wasm {
                let guest_t0 = Instant::now();
                match wasm.on_tick(dt) {
                    Ok(packed) => {
                        let (ptr, len) = decode_packed(packed);
                        if len > engine_wasm::MAX_OUTBOUND_DELTA_SIZE {
                            tracing::warn!(
                                "[sim] ⚠️ delta size {len} exceeds ABI max — skipped dispatch"
                            );
                            trap = true;
                            break;
                        }
                        if len > 0 {
                            if let Err(e) =
                                wasm.read_memory_into(ptr as u32, len as u32, &mut self.delta_buf)
                            {
                                tracing::error!("[sim] ⚠️ delta read failed: {e}");
                                trap = true;
                                break;
                            }
                        } else {
                            self.delta_buf.clear();
                        }
                    }
                    Err(e) if e.is_interrupt() => {
                        self.recorder.tick_overruns += 1;
                        tracing::warn!(
                            "[sim] ⛔ watchdog cut off on_tick at room tick {} after {}ms; the room stays up and the next call re-arms, but its state may be torn",
                            self.room.current_tick,
                            watchdog.tick_budget_ms
                        );
                        trap = true;
                        break;
                    }
                    Err(e) => {
                        tracing::warn!(
                            "[sim] ⚠️ guest trap in on_tick at room tick {}: {e}",
                            self.room.current_tick
                        );
                        trap = true;
                        break;
                    }
                }
                self.stage_guest_us_sum += guest_t0.elapsed().as_micros() as u64;
            } else {
                self.delta_buf.clear();
            }
        }
        self.trap = trap;
        ticks_to_run
    }

    pub(super) fn finish_frame(&mut self, ticks_to_run: u32) {
        if !self.trap && !self.delta_buf.is_empty() {
            self.emit_deltas();
        }
        let t0 = self.tick_t0;
        self.stage_sim_us_sum += t0.elapsed().as_micros() as u64;
        self.window_ticks += ticks_to_run as u64;
        self.recorder.note_tick(t0.elapsed());
        self.maintenance();
        let _ = self.native_send_failed;
    }
}
