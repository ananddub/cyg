//! The once-a-second publish: roll the windows, stamp the report, feed the
//! bus and the process-wide `current()` slot.

use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};

use arc_swap::ArcSwapOption;
use engine_bus::{Bus, StatusSnapshot, TICK_HISTOGRAM_BUCKETS};
use engine_dashboard::MAX_CLIENT_ROWS;

use super::recorder::Recorder;
use super::report::PublishReport;
use super::stats::nominal_tick_us;

impl Recorder {
    /// Once per second: `snapshot()` plus the caller's session report →
    /// eprintln JSON on `stdout` (the DoD's latency report) and `try_send` to
    /// `bus.status_tx` for the Tokio plane. Returns `true` when a snapshot
    /// went out, so the caller knows whether its own window rolled over.
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
        snap.ws_sessions = report.ws_sessions;
        snap.webrtc_sessions = report.webrtc_sessions;
        snap.udp_sessions = report.udp_sessions;
        snap.empty_for_sec = if report.players > 0
            || report.ws_sessions > 0
            || report.webrtc_sessions > 0
            || report.udp_sessions > 0
        {
            self.empty_since = None;
            0
        } else {
            // Armed on the first empty window, so the count starts the second
            // the room lost its last session rather than at process start.
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
        tracing::info!("[status] {}", serde_json::json!(&snap));

        // Send-or-drop. `bus.status_rx` belongs to `status::pump`, which claims
        // it exactly once, so a full queue means the consumer stopped, not that
        // it is behind — and `/status` says so by ageing the sample it holds.
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

/// Last snapshot published (for any main-thread reader; lock-free read).
pub fn current() -> Option<StatusSnapshot> {
    LAST.get_or_init(ArcSwapOption::default)
        .load()
        .as_deref()
        .cloned()
}
