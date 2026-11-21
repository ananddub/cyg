use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64};

use crossbeam_channel::bounded;

use super::*;

impl Bus {
    pub fn new(input_cap: usize, delta_cap: usize) -> Arc<Bus> {
        let (inputs_tx, inputs_rx) = bounded(input_cap);
        let (commands_tx, commands_rx) = bounded(COMMAND_CAP);
        let (deltas_tx, deltas_rx) = bounded(delta_cap);
        let (status_tx, status_rx) = bounded(STATUS_CAP);
        let (reloads_tx, reloads_rx) = bounded(RELOAD_CAP);
        let (map_reloads_tx, map_reloads_rx) = bounded(MAP_RELOAD_CAP);
        let (settings_tx, settings_rx) = bounded(SETTINGS_CAP);
        let (events_tx, events_rx) = bounded(EVENT_CAP);
        let (matches_tx, matches_rx) = bounded(MATCH_CAP);
        let (snapshots_tx, snapshots_rx) = bounded(SNAPSHOT_CAP);
        let (fetches_tx, fetches_rx) = bounded(FETCH_CAP);
        let (ownership_tx, ownership_rx) = bounded(OWNERSHIP_CAP);
        let (seats_tx, seats_rx) = bounded(SEAT_CAP);
        Arc::new(Bus {
            inputs_tx,
            inputs_rx,
            inputs_dropped: AtomicU64::new(0),
            commands_tx,
            commands_rx,
            deltas_tx,
            deltas_rx,
            status_tx,
            status_rx,
            reloads_tx,
            reloads_rx,
            map_reloads_tx,
            map_reloads_rx,
            settings_tx,
            settings_rx,
            events_tx,
            events_rx,
            events_dropped: AtomicU64::new(0),
            matches_tx,
            matches_rx,
            matches_dropped: AtomicU64::new(0),
            snapshots_tx,
            snapshots_rx,
            fetches_tx,
            fetches_rx,
            ownership_tx,
            ownership_rx,
            ownership_dropped: AtomicU64::new(0),
            seats_tx,
            seats_rx,
            seats_dropped: AtomicU64::new(0),
            draining: AtomicBool::new(false),
        })
    }
}
