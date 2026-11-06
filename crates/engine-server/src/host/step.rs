use std::time::{Duration, Instant};

use crate::host::*;

impl<'a> Engine<'a> {
    /// Returns true when the process should exit (drain grace elapsed and
    /// rooms are closed). The run loop used to ignore this and spin forever
    /// re-running `drain_rooms` every tick at 100% CPU.
    pub(crate) fn step(&mut self) -> std::io::Result<bool> {
        if self.drain_phase()? {
            return Ok(true);
        }
        self.pump_signals();
        self.wait_events();
        self.dispatch_events();
        {
            let mut ctx = UdpCtx {
                server: &mut self.server,
                transport: &mut self.transport,
                bus: &self.bus,
                game_cfg: &self.game_cfg,
                cfg: self.cfg,
                recorder: &mut self.stats.recorder,
                tick_interval: self.clock.tick_interval,
            };
            self.udp.run(&mut ctx);
            self.udp.pump();
        }
        self.run_inbound();
        {
            let mut ctx = TickCtx {
                server: &mut self.server,
                transport: &mut self.transport,
                udp: &mut self.udp,
                bus: &self.bus,
                game_cfg: &self.game_cfg,
                cfg: self.cfg,
                game_id: &self.game_id,
                node_id: &self.node_id,
                recorder: &mut self.stats.recorder,
                tick_interval: self.clock.tick_interval,
            };
            self.tick.run(&mut ctx);
        }
        self.run_telemetry();
        self.hot
            .run(&mut self.server, &self.bus, self.cfg, &mut self.clock.tick_interval);
        self.transport.flush().expect("flush");
        Ok(false)
    }

    pub(crate) fn drain_phase(&mut self) -> std::io::Result<bool> {
        if !self.bus.is_draining() {
            return Ok(false);
        }
        let now = Instant::now();
        if self.drain.begin(now) {
            tracing::info!(
                "[server] 🛑 drain requested: {} room(s) get {:?} to finish and leave",
                self.server.rooms.len(),
                self.drain.grace
            );
            announce_closure(&self.server, &mut self.transport);
        }
        if self.drain.expired(Instant::now()) {
            drain_rooms(
                &mut self.server,
                &mut self.transport,
                &self.bus,
                self.cfg,
                &self.game_id,
            );
            return Ok(true);
        }
        Ok(false)
    }

    pub(crate) fn wait_events(&mut self) {
        if !self.scratch.events.is_empty() {
            return;
        }
        let remaining_to_tick = self
            .clock
            .tick_interval
            .saturating_sub(self.tick.last_tick.elapsed());
        let all_idle = self
            .server
            .rooms
            .values()
            .all(|r| r.is_hibernating())
            && self.udp.udp_peers.is_empty();
        let wait = if all_idle {
            remaining_to_tick.min(Duration::from_millis(5))
        } else {
            remaining_to_tick
        };
        if let Err(e) = self.transport.poll_events(wait, &mut self.scratch.events) {
            if e.kind() != std::io::ErrorKind::Interrupted {
                panic!("poll: {e}");
            }
        }
    }
}
