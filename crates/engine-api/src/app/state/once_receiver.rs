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
}
