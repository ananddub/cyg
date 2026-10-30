use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use crossbeam_channel::{Receiver, Sender, bounded};

use crate::messages::{
    InputMsg, MapReload, MatchFinished, OutFrame, RoomSnapshot, SettingsReload, SimCommand,
    SimEvent, WasmReload,
};
use crate::telemetry::StatusSnapshot;

const COMMAND_CAP: usize = 256;
const STATUS_CAP: usize = 8;
/// Hot-reload requests are rare and must never be dropped by a busy input path;
/// a small dedicated queue keeps them independent. Each entry now carries the
/// module bytes (the console caps an upload at 16 MiB), so the slot count is
/// deliberately tiny: four in-flight modules bound the worst case at 64 MiB.
const RELOAD_CAP: usize = 4;
/// Map activations are rarer still; one queue so a map flip never sits behind
/// wasm reloads.
const MAP_RELOAD_CAP: usize = 8;
/// Console settings changes (tick rate) — rare by nature.
const SETTINGS_CAP: usize = 8;
/// Gameplay events waiting for the control plane to write them. The tick keeps
/// moving regardless, so this queue is where a slow database shows up as dropped
/// history instead of dropped frames. An event payload is one guest
/// `host_send` body, bounded upstream at 64 KiB, so the worst case here is
/// ~16 MiB of retained bytes.
const EVENT_CAP: usize = 256;
/// Finished matches waiting to be written. One row per room, at the moment the
/// room is disposed, so a small cap is generous: it only has to survive the
/// control plane being briefly busy.
const MATCH_CAP: usize = 64;
/// Saved room states waiting for the database. A snapshot is taken when a room
/// is disposed, so at most one per room per pass; eight rooms closing inside one
/// 250 ms pump beat is already a backlog an operator wants to see.
const SNAPSHOT_CAP: usize = 8;

/// Inter-plane wiring. Each plane consumes its own ends directly; sends on
/// `inputs` must go through [`Bus::push_input`] or the drop policy is lost.
pub struct Bus {
    /// planes → sim
    pub inputs_tx: Sender<InputMsg>,
    pub inputs_rx: Receiver<InputMsg>,
    /// planes/control → sim
    pub commands_tx: Sender<SimCommand>,
    pub commands_rx: Receiver<SimCommand>,
    /// sim → browser plane
    pub deltas_tx: Sender<OutFrame>,
    pub deltas_rx: Receiver<OutFrame>,
    /// sim → status sink, latest-wins
    pub status_tx: Sender<StatusSnapshot>,
    pub status_rx: Receiver<StatusSnapshot>,
    /// control plane → sim/host: WASM hot-reload requests (upload / rollback),
    /// drained by the host loop's module watcher.
    pub reloads_tx: Sender<WasmReload>,
    pub reloads_rx: Receiver<WasmReload>,
    /// control plane → sim/host: the active map changed (upload / activate),
    /// drained by the host loop (applies at next room spawn).
    pub map_reloads_tx: Sender<MapReload>,
    pub map_reloads_rx: Receiver<MapReload>,
    /// control plane → sim/host: a live-tunable setting changed (tick rate).
    pub settings_tx: Sender<SettingsReload>,
    pub settings_rx: Receiver<SettingsReload>,
    /// sim → control plane: gameplay events to persist. Sending never blocks the
    /// tick; see [`Bus::push_event`].
    pub events_tx: Sender<SimEvent>,
    pub events_rx: Receiver<SimEvent>,
    events_dropped: AtomicU64,
    /// sim → control plane: matches that ended, for the history table. Same
    /// never-block-the-tick rule as [`Bus::push_event`].
    pub matches_tx: Sender<MatchFinished>,
    pub matches_rx: Receiver<MatchFinished>,
    matches_dropped: AtomicU64,
    /// sim → control plane: saved room states to persist. See
    /// [`Bus::push_snapshot`].
    pub snapshots_tx: Sender<RoomSnapshot>,
    pub snapshots_rx: Receiver<RoomSnapshot>,
    /// Latched by whatever catches the process' termination signal and read by
    /// the loop that owns the rooms. A one-way latch, so an atomic beats a
    /// channel that every consumer would have to keep draining.
    draining: AtomicBool,
}

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
        Arc::new(Bus {
            inputs_tx,
            inputs_rx,
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
            draining: AtomicBool::new(false),
        })
    }

    /// Tell every plane that the process is going away. Idempotent: SIGINT and
    /// SIGTERM, or two watchers, latches the same flag.
    pub fn request_drain(&self) {
        self.draining.store(true, Ordering::Relaxed);
    }

    /// Whether the host loop should flush its rooms and return.
    pub fn is_draining(&self) -> bool {
        self.draining.load(Ordering::Relaxed)
    }

    /// On full, drops the oldest queued input and retries once — never the
    /// fresh one.
    pub fn push_input(&self, msg: InputMsg) {
        let msg = match self.inputs_tx.try_send(msg) {
            Ok(()) => return,
            Err(crossbeam_channel::TrySendError::Full(m)) => m,
            Err(_) => return,
        };
        let _ = self.inputs_rx.try_recv();
        let _ = self.inputs_tx.try_send(msg);
    }

    /// Queue one gameplay event for the control plane. On full this drops the
    /// **oldest** queued event and retries, exactly like
    /// [`push_input`](Self::push_input): history is best-effort, the tick is
    /// not, and the newest event is the one an operator still wants to see.
    /// Every loss counts on [`events_dropped`](Self::events_dropped).
    pub fn push_event(&self, event: SimEvent) {
        let event = match self.events_tx.try_send(event) {
            Ok(()) => return,
            Err(crossbeam_channel::TrySendError::Full(e)) => e,
            Err(_) => return,
        };
        let _ = self.events_rx.try_recv();
        self.events_dropped.fetch_add(1, Ordering::Relaxed);
        let _ = self.events_tx.try_send(event);
    }

    /// Events discarded because the control plane was not draining the queue.
    pub fn events_dropped(&self) -> u64 {
        self.events_dropped.load(Ordering::Relaxed)
    }

    /// Queue one finished match for the history table. Disposing a room is not
    /// the tick's hot path, but it is on the loop that closes rooms, so this
    /// keeps the same non-blocking contract as [`push_event`](Self::push_event).
    pub fn push_match(&self, match_: MatchFinished) {
        let match_ = match self.matches_tx.try_send(match_) {
            Ok(()) => return,
            Err(crossbeam_channel::TrySendError::Full(m)) => m,
            Err(_) => return,
        };
        let _ = self.matches_rx.try_recv();
        self.matches_dropped.fetch_add(1, Ordering::Relaxed);
        let _ = self.matches_tx.try_send(match_);
    }

    /// Matches discarded because the control plane was not draining the queue.
    pub fn matches_dropped(&self) -> u64 {
        self.matches_dropped.load(Ordering::Relaxed)
    }

    /// Hand one saved room state to the control plane. A snapshot is written
    /// when a room is disposed — at most one per room — so this queue is allowed
    /// to be blunt: on full it drops the **oldest** saved state, which is the
    /// one a newer save has already superseded.
    pub fn push_snapshot(&self, snapshot: RoomSnapshot) {
        let snapshot = match self.snapshots_tx.try_send(snapshot) {
            Ok(()) => return,
            Err(crossbeam_channel::TrySendError::Full(s)) => s,
            Err(_) => return,
        };
        let _ = self.snapshots_rx.try_recv();
        let _ = self.snapshots_tx.try_send(snapshot);
    }
}

#[cfg(test)]
mod tests;
