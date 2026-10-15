//! A one-shot wrapper around the bus's status channel.

use std::sync::Mutex;

use crossbeam_channel::Receiver;
use engine_bus::StatusSnapshot;

/// A receiver that can be handed to the router but must not be duplicated:
/// two consumers of `bus.status_rx` would each see half the samples.
#[derive(Default)]
pub struct OnceReceiver {
    inner: Mutex<Option<Receiver<StatusSnapshot>>>,
}

impl OnceReceiver {
    pub fn new(rx: Receiver<StatusSnapshot>) -> OnceReceiver {
        OnceReceiver {
            inner: Mutex::new(Some(rx)),
        }
    }

    /// `None` once taken — the pump stops and `/status` keeps reporting the
    /// last sample it saw rather than failing.
    pub fn try_take(&self) -> Option<Receiver<StatusSnapshot>> {
        self.inner.lock().ok()?.take()
    }
}
