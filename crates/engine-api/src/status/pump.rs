//! The single consumer of the sim's status queue.

use std::time::Duration;

use crate::ApiState;

/// How long the pump sleeps between drains. The sim publishes once a second,
/// so this only bounds how stale a sample can look after the sender wakes up.
const PUMP_INTERVAL: Duration = Duration::from_millis(250);

/// The one consumer of `bus.status_rx`.
///
/// Returns immediately if someone else already took the queue: `/status` then
/// keeps reporting the last sample it ever saw instead of failing.
pub async fn pump(state: ApiState) {
    let Some(rx) = state.status_rx.try_take() else {
        eprintln!("[control] status queue already owned; /status serves the last sample");
        return;
    };
    // `crossbeam_channel::Receiver` is not awaitable, so this polls on a
    // quarter-second tick; polling decides how fast a fresh sample appears,
    // never whether it is lost.
    let mut ticker = tokio::time::interval(PUMP_INTERVAL);
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    loop {
        ticker.tick().await;
        // Drain everything queued and keep the newest: a plane that slept for
        // 5 s must not report a 5 s-old sample as current.
        let mut newest = None;
        while let Ok(snap) = rx.try_recv() {
            newest = Some(snap);
        }
        if let Some(snap) = newest
            && let Ok(mut slot) = state.latest_status.lock()
        {
            crate::dashboard::record_sample(snap.clone());
            *slot = Some(snap);
        }
    }
}
