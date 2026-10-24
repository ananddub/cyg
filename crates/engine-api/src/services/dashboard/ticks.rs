use engine_dashboard::{ClientRow, MAX_CLIENT_ROWS, TICK_HISTOGRAM_BUCKETS, TickPoint, TickSeries};

use crate::ApiState;
use crate::utils::dashboard::history::history;

pub fn build_clients(state: &ApiState) -> Vec<ClientRow> {
    let room = state.game.name.clone();
    let Some(entry) = history().newest() else {
        return Vec::new();
    };
    entry
        .snapshot
        .sessions
        .iter()
        .take(MAX_CLIENT_ROWS)
        .map(|s| ClientRow {
            session: s.session.clone(),
            transport: s.transport.clone(),
            room: room.clone(),
            player_id: s.player_id,
            acked_seq: s.acked_seq,
            idle_ms: s.idle_ms,
        })
        .collect()
}

pub fn build_ticks() -> TickSeries {
    let entries = history().snapshot_all();
    let mut prev_ms: Option<u64> = None;
    let points = entries
        .iter()
        .map(|e| {
            // sim publishes 1/s; >2.5 s between arrivals means a lost sample -> break the chart line
            let gap = prev_ms.is_some_and(|p| e.arrived_ms.saturating_sub(p) > 2_500);
            prev_ms = Some(e.arrived_ms);
            TickPoint {
                at_ms: e.arrived_ms,
                p50_us: e.snapshot.tick_budget_p50_us,
                p99_us: e.snapshot.tick_budget_p99_us,
                hz: e.snapshot.tick_hz_est,
                players: e.snapshot.players,
                seq: e.snapshot.seq,
                gap,
            }
        })
        .collect();
    let histogram = entries
        .last()
        .map(|e| e.snapshot.tick_histogram)
        .unwrap_or([0; TICK_HISTOGRAM_BUCKETS]);
    TickSeries {
        interval_sec: 1,
        points,
        histogram,
        bucket_edges_us: engine_dashboard::bucket_edges_us(),
    }
}
