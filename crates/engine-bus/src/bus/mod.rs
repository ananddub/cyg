use std::sync::atomic::{AtomicBool, AtomicU64};
use std::time::Duration;

use crossbeam_channel::{Receiver, Sender, bounded};

use crate::messages::{
    InputMsg, MapReload, MatchFinished, OutFrame, RoomOwnership, RoomSnapshot, SeatUpdate,
    SettingsReload, SimCommand, SimEvent, SnapshotFetch, WasmReload,
};
use crate::telemetry::StatusSnapshot;

mod construct;
mod push;

const COMMAND_CAP: usize = 256;
const STATUS_CAP: usize = 8;

const RELOAD_CAP: usize = 4;

const MAP_RELOAD_CAP: usize = 8;

const SETTINGS_CAP: usize = 8;

const EVENT_CAP: usize = 256;

const MATCH_CAP: usize = 64;

const SNAPSHOT_CAP: usize = 8;

const FETCH_CAP: usize = 8;

const OWNERSHIP_CAP: usize = 32;

const SEAT_CAP: usize = 256;

pub struct Bus {
    pub inputs_tx: Sender<InputMsg>,
    pub inputs_rx: Receiver<InputMsg>,
    inputs_dropped: AtomicU64,

    pub commands_tx: Sender<SimCommand>,
    pub commands_rx: Receiver<SimCommand>,

    pub deltas_tx: Sender<OutFrame>,
    pub deltas_rx: Receiver<OutFrame>,

    pub status_tx: Sender<StatusSnapshot>,
    pub status_rx: Receiver<StatusSnapshot>,

    pub reloads_tx: Sender<WasmReload>,
    pub reloads_rx: Receiver<WasmReload>,

    pub map_reloads_tx: Sender<MapReload>,
    pub map_reloads_rx: Receiver<MapReload>,

    pub settings_tx: Sender<SettingsReload>,
    pub settings_rx: Receiver<SettingsReload>,

    pub events_tx: Sender<SimEvent>,
    pub events_rx: Receiver<SimEvent>,
    events_dropped: AtomicU64,

    pub matches_tx: Sender<MatchFinished>,
    pub matches_rx: Receiver<MatchFinished>,
    matches_dropped: AtomicU64,

    pub snapshots_tx: Sender<RoomSnapshot>,
    pub snapshots_rx: Receiver<RoomSnapshot>,

    pub fetches_tx: Sender<SnapshotFetch>,
    pub fetches_rx: Receiver<SnapshotFetch>,

    pub ownership_tx: Sender<RoomOwnership>,
    pub ownership_rx: Receiver<RoomOwnership>,
    ownership_dropped: AtomicU64,

    pub seats_tx: Sender<SeatUpdate>,
    pub seats_rx: Receiver<SeatUpdate>,
    seats_dropped: AtomicU64,

    draining: AtomicBool,
}

impl Bus {
    pub fn fetch_snapshot(&self, room: &str, wait: Duration) -> Option<RoomSnapshot> {
        let (reply_tx, reply_rx) = bounded(1);
        if self
            .fetches_tx
            .try_send(SnapshotFetch {
                room: room.to_string(),
                reply: reply_tx,
            })
            .is_err()
        {
            return None;
        }
        reply_rx.recv_timeout(wait).ok().flatten()
    }
}

#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_persistence;
