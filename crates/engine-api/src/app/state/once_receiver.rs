use std::sync::Mutex;

use crossbeam_channel::Receiver;
use engine_bus::StatusSnapshot;

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

    pub fn try_take(&self) -> Option<Receiver<StatusSnapshot>> {
        self.inner.lock().ok()?.take()
    }

    /// Non-consuming peek. The status pump takes the queue once and keeps it
    /// forever, so a taken receiver is how `/readyz` knows the pump is alive.
    pub fn is_taken(&self) -> bool {
        self.inner.lock().is_ok_and(|guard| guard.is_none())
    }
}
