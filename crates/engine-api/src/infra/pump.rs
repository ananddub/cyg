use std::time::Duration;

use crate::ApiState;

const PUMP_INTERVAL: Duration = Duration::from_millis(250);

pub async fn pump(state: ApiState) {
    let Some(rx) = state.status_rx.try_take() else {
        tracing::info!("[control] status queue already owned; /status serves the last sample");
        return;
    };
    let mut ticker = tokio::time::interval(PUMP_INTERVAL);
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    loop {
        ticker.tick().await;
        let mut newest = None;
        while let Ok(snap) = rx.try_recv() {
            newest = Some(snap);
        }
        if let Some(snap) = newest
            && let Ok(mut slot) = state.latest_status.lock()
        {
            crate::utils::dashboard::history::record_sample(snap.clone());
            *slot = Some(snap);
        }
    }
}
