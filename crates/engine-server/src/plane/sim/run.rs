use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use engine_bus::Bus;

use crate::plane::sim::state::Plane;
use crate::plane::{self, PlaneConfig, tick_period};
use crate::telemetry::Recorder;

pub fn run(bus: Arc<Bus>, cfg: PlaneConfig, stop: Arc<AtomicBool>) {
    let Some((udp, poll, room, _epoch_ticker)) = plane::sim::boot::boot(Arc::clone(&bus), &cfg)
    else {
        return;
    };

    tracing::info!(
        "[sim] running on core {} tick_hz={} wasm={}",
        plane::current_core(),
        cfg.tick_hz,
        cfg.wasm_path.display()
    );

    let period = tick_period(cfg.tick_hz);
    let dt = period.as_secs_f64();
    let mut plane = Plane {
        bus,
        cfg,
        udp,
        poll,
        room,
        _epoch_ticker,
        recorder: Recorder::new(),
        events: mio::Events::with_capacity(32),
        period,
        dt,
        aoi_cfg: engine_config::AoiConfig::default(),
        plane_game_cfg: engine_config::GameConfig::default(),
        node_id: engine_config::node_id(),
        last_tick: std::time::Instant::now(),
        sweep_start: std::time::Instant::now(),
        status_start: std::time::Instant::now(),
        window_ticks: 0,
        stage_sim_us_sum: 0,
        stage_guest_us_sum: 0,
        measured_hz: 128.0,
        native_send_failed: 0,
        browser_players: Default::default(),
        session_of: Default::default(),
        last_input_at: Default::default(),
        last_pos: Default::default(),
        delta_buf: Vec::with_capacity(8 * 1024),
        filtered_buf: Vec::with_capacity(8 * 1024),
        visible_buf: Vec::with_capacity(256),
        scratch: Vec::with_capacity(8 * 1024),
        frames: Vec::new(),
        tick_t0: std::time::Instant::now(),
        trap: false,
    };
    plane.measured_hz = plane.cfg.tick_hz;

    while !stop.load(Ordering::Relaxed) {
        plane.step();
    }
    tracing::info!("[sim] stop requested — plane loop exited");
}

impl Plane {
    fn step(&mut self) {
        let timeout = self.period.saturating_sub(self.last_tick.elapsed());
        if let Err(e) = self.poll.poll(&mut self.events, Some(timeout)) {
            tracing::error!("[sim] ⚠️ mio poll error: {e}");
        }
        for ev in self.events.iter() {
            if ev.token() == plane::udp::UDP_TOKEN && ev.is_readable() {
                self.udp.on_readable();
            }
        }

        self.drain_commands();
        self.udp.drain_pending();
        self.udp.pump_reliable();
        self.drain_inputs();

        self.room.note_state();
        let elapsed = self.last_tick.elapsed();
        if elapsed < self.period {
            self.quiet_frame();
            return;
        }
        let ticks_to_run = self.run_ticks(elapsed);
        self.finish_frame(ticks_to_run);
    }
}
