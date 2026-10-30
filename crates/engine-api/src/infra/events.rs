//! The control plane's half of the host's event mirror. The sim queues listed
//! guest messages and its own desync evidence on the bus; this drains them into
//! rows and ages the history out. Nothing here may slow the sim: its queue is
//! bounded and drops its oldest row, and the `seq` gaps a reader sees are how
//! that loss becomes visible instead of silent.

use std::time::Duration;

use chrono::Utc;
use uuid::Uuid;

use crate::ApiState;
use crate::repositories::models::{DesyncReport, GameplayEvent};

const PUMP_INTERVAL: Duration = Duration::from_millis(250);
const SWEEP_INTERVAL: Duration = Duration::from_secs(3600);
/// The engine's own event: two checksums that disagreed, little-endian.
const DESYNC_EVENT: &str = "desync";
const CHECKSUM_PAIR_LEN: usize = 16;

fn checksum_pair(payload: &[u8]) -> Option<(u64, u64)> {
    if payload.len() != CHECKSUM_PAIR_LEN {
        return None;
    }
    let word = |at: usize| {
        let mut a = [0u8; 8];
        a.copy_from_slice(&payload[at..at + 8]);
        u64::from_le_bytes(a)
    };
    Some((word(0), word(8)))
}

pub async fn pump_events(state: ApiState) {
    let Some(q) = state.q.clone() else {
        tracing::info!("[control] no datastore; mirrored events are dropped on the floor");
        return;
    };
    // One pump owns the queue. A second consumer would split it, and every
    // event would land in whichever task happened to be polling.
    let rx = state.bus.events_rx.clone();
    let mut ticker = tokio::time::interval(PUMP_INTERVAL);
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    loop {
        ticker.tick().await;
        while let Ok(event) = rx.try_recv() {
            let desync = if event.event == DESYNC_EVENT {
                checksum_pair(&event.payload)
            } else {
                None
            };
            let stored = match desync {
                Some((expected, client)) => {
                    q.record_desync_report(&DesyncReport {
                        id: Uuid::new_v4(),
                        room: event.room,
                        player_id: event.target,
                        tick: event.tick,
                        expected_checksum: format!("{expected:016x}"),
                        client_checksum: format!("{client:016x}"),
                        create_time: Utc::now(),
                    })
                    .await
                }
                None => {
                    q.record_gameplay_event(&GameplayEvent {
                        id: Uuid::new_v4(),
                        room: event.room,
                        tick: event.tick,
                        seq: event.seq as i64,
                        player_id: event.target,
                        event: event.event,
                        payload: event.payload,
                        create_time: Utc::now(),
                    })
                    .await
                }
            };
            if let Err(err) = stored {
                tracing::warn!("[control] dropped a mirrored event: {err}");
            }
        }
    }
}

/// A match nobody has read for the retention window will not be read later, and
/// these rows are the bulk of what the engine writes.
pub async fn sweep_match_history(state: ApiState) {
    let Some(q) = state.q.clone() else {
        return;
    };
    let mut ticker = tokio::time::interval(SWEEP_INTERVAL);
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    loop {
        ticker.tick().await;
        let days = i64::from(state.game.gameplay.event_ttl_days.max(1));
        let cutoff = Utc::now() - chrono::Duration::days(days);
        if let Err(err) = q.prune_match_history(cutoff).await {
            tracing::warn!("[control] match history prune failed: {err}");
        }
    }
}
