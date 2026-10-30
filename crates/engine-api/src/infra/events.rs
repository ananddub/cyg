//! The control plane's half of the host's event mirror. The sim queues listed
//! guest messages and its own desync evidence on the bus; this drains them into
//! rows and ages the history out. Nothing here may slow the sim: its queue is
//! bounded and drops its oldest row, and the `seq` gaps a reader sees are how
//! that loss becomes visible instead of silent.

use std::time::Duration;

use chrono::{DateTime, Utc};
use engine_bus::RoomSnapshot;
use uuid::Uuid;

use crate::ApiState;
use crate::repositories::models::{DesyncReport, GameplayEvent, MatchHistory};

const PUMP_INTERVAL: Duration = Duration::from_millis(250);
const SWEEP_INTERVAL: Duration = Duration::from_secs(3600);
/// The engine's own event: two checksums that disagreed, little-endian.
const DESYNC_EVENT: &str = "desync";
const CHECKSUM_PAIR_LEN: usize = 16;

/// The host reports wall seconds because an `Instant` cannot survive a restart.
fn stamp(secs: i64) -> DateTime<Utc> {
    DateTime::from_timestamp(secs, 0).unwrap_or_else(Utc::now)
}

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
    let matches_rx = state.bus.matches_rx.clone();
    let snapshots_rx = state.bus.snapshots_rx.clone();
    let fetches_rx = state.bus.fetches_rx.clone();
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
                    let report = DesyncReport {
                        id: Uuid::new_v4(),
                        room: event.room,
                        player_id: event.target,
                        tick: event.tick,
                        expected_checksum: format!("{expected:016x}"),
                        client_checksum: format!("{client:016x}"),
                        create_time: Utc::now(),
                    };
                    let summary = serde_json::json!({
                        "report_id": report.id,
                        "room": report.room,
                        "player_id": report.player_id,
                        "tick": report.tick,
                        "expected_checksum": report.expected_checksum,
                        "client_checksum": report.client_checksum,
                    });
                    let key = report.id.to_string();
                    let stored = q.record_desync_report(&report).await;
                    if stored.is_ok() {
                        crate::services::webhook::emit(&state, "report.created", &key, summary)
                            .await;
                    }
                    stored
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
        // A finished match is one row per room, and the room is already gone by
        // the time it arrives: losing it is a hole in the ledger, not a stall.
        while let Ok(finished) = matches_rx.try_recv() {
            if let Err(err) = q
                .record_match(&MatchHistory {
                    id: Uuid::new_v4(),
                    game_id: finished.game_id,
                    room: finished.room,
                    end_reason: finished.end_reason,
                    started_at: stamp(finished.started_at_unix),
                    ended_at: stamp(finished.ended_at_unix),
                    ticks: finished.ticks,
                    peak_players: finished.peak_players,
                    snapshot_seq: finished.snapshot_seq,
                    result: finished.result,
                    create_time: Utc::now(),
                })
                .await
            {
                tracing::warn!("[control] dropped a finished match: {err}");
            }
        }
        // A snapshot is the room's current state, so the queue holds at most one
        // per room and an older one is worth nothing — superseding it is correct,
        // and writing the bytes a node is about to lose is the whole point.
        while let Ok(saved) = snapshots_rx.try_recv() {
            if let Err(err) = q
                .save_room_snapshot(&saved.room, &saved.backend, &saved.bytes, 0, saved.seq)
                .await
            {
                tracing::warn!("[control] dropped a room snapshot: {err}");
            }
        }
        // A host spawning a room it has never seen asks for the state an earlier
        // run saved. It is waiting on the answer, so a lookup that failed is
        // answered with `None` rather than left to time out: "start empty" is
        // the same outcome the host reaches on its own, and it reaches it sooner.
        while let Ok(fetch) = fetches_rx.try_recv() {
            let found = match q.load_room_snapshot(&fetch.room).await {
                Ok(Some(row)) => Some(RoomSnapshot {
                    room: row.room_name,
                    backend: row.backend,
                    seq: row.snapshot_seq,
                    bytes: row.snapshot_bytes,
                }),
                Ok(None) => None,
                Err(err) => {
                    tracing::warn!("[control] snapshot fetch for '{}' failed: {err}", fetch.room);
                    None
                }
            };
            let _ = fetch.reply.send(found);
        }
    }
}

/// History nobody reads inside the retention window will not be read later, and
/// these rows are the bulk of what the engine writes. Each table ages on its own
/// window: match summaries outlive the event stream they came from.
pub async fn sweep_match_history(state: ApiState) {
    let Some(q) = state.q.clone() else {
        return;
    };
    let mut ticker = tokio::time::interval(SWEEP_INTERVAL);
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    loop {
        ticker.tick().await;
        let cutoff = |days: u32| Utc::now() - chrono::Duration::days(i64::from(days.max(1)));
        if let Err(err) = q
            .prune_event_history(cutoff(state.game.gameplay.event_ttl_days))
            .await
        {
            tracing::warn!("[control] event history prune failed: {err}");
        }
        if let Err(err) = q
            .prune_match_history(cutoff(state.game.gameplay.match_history_ttl_days))
            .await
        {
            tracing::warn!("[control] match history prune failed: {err}");
        }
    }
}
