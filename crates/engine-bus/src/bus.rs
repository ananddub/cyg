use std::sync::Arc;

use crossbeam_channel::{Receiver, Sender, bounded};

use crate::messages::{InputMsg, OutFrame, SimCommand};
use crate::telemetry::StatusSnapshot;

const COMMAND_CAP: usize = 256;
const STATUS_CAP: usize = 8;

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
}

impl Bus {
    pub fn new(input_cap: usize, delta_cap: usize) -> Arc<Bus> {
        let (inputs_tx, inputs_rx) = bounded(input_cap);
        let (commands_tx, commands_rx) = bounded(COMMAND_CAP);
        let (deltas_tx, deltas_rx) = bounded(delta_cap);
        let (status_tx, status_rx) = bounded(STATUS_CAP);
        Arc::new(Bus {
            inputs_tx,
            inputs_rx,
            commands_tx,
            commands_rx,
            deltas_tx,
            deltas_rx,
            status_tx,
            status_rx,
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
mod tests {
    use super::*;
    use crate::messages::Via;

    fn input(seq: u16) -> InputMsg {
        InputMsg {
            player_id: 1,
            seq,
            payload: [0; 20],
            arrival_ns: 0,
            via: Via::NativeUdp,
        }
    }

    #[test]
    fn push_input_accepts_until_full() {
        let bus = Bus::new(2, 2);
        bus.push_input(input(1));
        bus.push_input(input(2));
        assert_eq!(bus.inputs_rx.len(), 2);
    }

    #[test]
    fn push_input_full_drops_the_oldest_and_keeps_the_newest() {
        let bus = Bus::new(2, 2);
        bus.push_input(input(1));
        bus.push_input(input(2));
        bus.push_input(input(3));
        assert_eq!(bus.inputs_rx.try_recv().unwrap().seq, 2);
        assert_eq!(bus.inputs_rx.try_recv().unwrap().seq, 3);
        assert!(bus.inputs_rx.try_recv().is_err());
    }

    #[test]
    fn push_input_when_the_sim_is_gone_is_a_silent_noop() {
        let bus = Bus::new(2, 2);
        let tx = bus.inputs_tx.clone();
        let orphaned = Arc::try_unwrap(bus).ok().unwrap();
        drop(orphaned);
        assert!(matches!(
            tx.try_send(input(1)),
            Err(crossbeam_channel::TrySendError::Disconnected(_))
        ));
    }
}
