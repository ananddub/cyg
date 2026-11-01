
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};

use arc_swap::ArcSwapOption;
use engine_bus::{Bus, RoomPhase, StatusSnapshot, TICK_HISTOGRAM_BUCKETS};
use engine_dashboard::MAX_CLIENT_ROWS;

use super::recorder::Recorder;
use super::report::PublishReport;
use super::stats::nominal_tick_us;

impl Recorder {

    pub fn maybe_publish(&mut self, bus: &Arc<Bus>, report: PublishReport) -> bool {
        if self.last_publish.elapsed() < Duration::from_secs(1) {
            return false;
        }
        self.tick_hz = if report.tick_hz_est.is_finite() && report.tick_hz_est > 0.0 {
            report.tick_hz_est
        } else {
            128.0
        };
        let mut snap = self.snapshot();
        snap.players = report.players;
        snap.nominal_tick_us = nominal_tick_us(report.tick_rate_hz);
        snap.stage_sim_us = report.stage_sim_us;
        snap.stage_guest_us = report.stage_guest_us;
        snap.ws_sessions = report.ws_sessions;
        snap.webrtc_sessions = report.webrtc_sessions;
        snap.udp_sessions = report.udp_sessions;
        snap.rooms = report.rooms;

        for row in &snap.rooms {
            match row.phase {
                RoomPhase::Created => snap.rooms_created += 1,
                RoomPhase::Active => snap.rooms_active += 1,
                RoomPhase::Hibernating => snap.rooms_hibernating += 1,

                RoomPhase::Closed => {}
            }
        }
        snap.empty_for_sec = if report.players > 0
            || report.ws_sessions > 0
            || report.webrtc_sessions > 0
            || report.udp_sessions > 0
        {
            self.empty_since = None;
            0
        } else {

            let since = *self.empty_since.get_or_insert(Instant::now());
            since.elapsed().as_secs()
        };
        let cap = if report.session_cap == 0 {
            MAX_CLIENT_ROWS
        } else {
            report.session_cap.min(MAX_CLIENT_ROWS)
        };
        let mut sessions = report.sessions;
        sessions.truncate(cap);
        snap.sessions = sessions;
        self.publish_seq = self.publish_seq.saturating_add(1);
        snap.seq = self.publish_seq;

        tracing::debug!("[status] {}", serde_json::json!(&snap));

        let _ = bus.status_tx.try_send(snap.clone());
        LAST.get_or_init(ArcSwapOption::default).store(Some(Arc::new(snap)));

        self.n_input = 0;
        self.n_tick = 0;
        self.tick_hist = [0; TICK_HISTOGRAM_BUCKETS];
        self.last_publish = Instant::now();
        true
    }
}

static LAST: OnceLock<ArcSwapOption<StatusSnapshot>> = OnceLock::new();

pub fn current() -> Option<StatusSnapshot> {
    LAST.get_or_init(ArcSwapOption::default)
        .load()
        .as_deref()
        .cloned()
}
