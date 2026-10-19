use std::sync::Arc;

use crossbeam_channel::{Receiver, Sender, bounded};

use crate::messages::{InputMsg, OutFrame, SimCommand, WasmReload};
use crate::telemetry::StatusSnapshot;

const COMMAND_CAP: usize = 256;
const STATUS_CAP: usize = 8;
/// Hot-reload requests are rare and must never be dropped by a busy input path;
/// a small dedicated queue with its own capacity keeps them independent.
const RELOAD_CAP: usize = 32;

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
}

impl Bus {
    pub fn new(input_cap: usize, delta_cap: usize) -> Arc<Bus> {
        let (inputs_tx, inputs_rx) = bounded(input_cap);
        let (commands_tx, commands_rx) = bounded(COMMAND_CAP);
        let (deltas_tx, deltas_rx) = bounded(delta_cap);
        let (status_tx, status_rx) = bounded(STATUS_CAP);
        let (reloads_tx, reloads_rx) = bounded(RELOAD_CAP);
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
        })
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
}

#[cfg(test)]
mod tests;
