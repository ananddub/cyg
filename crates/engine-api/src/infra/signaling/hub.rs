//! The hub: route registry and the inbound hand-over slot.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

use tokio::sync::mpsc::{Receiver, Sender};

use super::consts::{INBOUND_QUEUE, mpsc_channel};
use super::frames::{SignalIn, SignalOut};
use super::replies::SeatReplies;

pub(super) struct RoutedQueue {
    /// Which socket generation owns this queue, so a reconnecting client's
    /// stale task cannot delete the route that replaced it.
    pub(super) attach_id: u64,
    pub(super) tx: Sender<SignalOut>,
}

/// The seam between this plane and whoever terminates WebRTC sessions. A
/// data-plane owner calls [`SignalHub::take_signals`] once, then [`SignalHub::publish`].
#[derive(Clone)]
pub struct SignalHub {
    pub(super) inbound_tx: Sender<SignalIn>,
    /// Handed over exactly once; a second consumer would see half the offers.
    pub(super) inbound_rx: Arc<std::sync::Mutex<Option<Receiver<SignalIn>>>>,
    pub(super) routes: Arc<std::sync::Mutex<HashMap<String, RoutedQueue>>>,
    ice_config: Arc<String>,
    pub(super) next_attach: Arc<AtomicU64>,
    pub(super) delivered: Arc<AtomicU64>,
    /// Replies refused for any reason (no live route, full queue, vanished
    /// socket). Counted, never blocking.
    pub(super) dropped_full: Arc<AtomicU64>,
}

impl SignalHub {
    pub fn new(ice_config: String) -> SignalHub {
        let (inbound_tx, inbound_rx) = mpsc_channel(INBOUND_QUEUE);
        SignalHub {
            inbound_tx,
            inbound_rx: Arc::new(std::sync::Mutex::new(Some(inbound_rx))),
            routes: Arc::new(std::sync::Mutex::new(HashMap::new())),
            ice_config: Arc::new(ice_config),
            next_attach: Arc::new(AtomicU64::new(1)),
            delivered: Arc::new(AtomicU64::new(0)),
            dropped_full: Arc::new(AtomicU64::new(0)),
        }
    }

    pub fn ice_config(&self) -> &str {
        &self.ice_config
    }

    /// The browser → data-plane queue. `None` if something already owns it.
    pub fn take_signals(&self) -> Option<Receiver<SignalIn>> {
        self.inbound_rx.lock().ok()?.take()
    }

    /// Open a reply queue for `seat`, before the upgrade, so an answer produced a
    /// microsecond later already has somewhere to go.
    pub fn attach(&self, seat: &str) -> SeatReplies {
        let (tx, rx) = mpsc_channel(super::consts::PER_SEAT_QUEUE);
        let attach_id = self.next_attach.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        if let Ok(mut routes) = self.routes.lock() {
            routes.insert(seat.to_string(), RoutedQueue { attach_id, tx });
        }
        SeatReplies {
            rx,
            seat: seat.to_string(),
            attach_id,
            hub: self.clone(),
        }
    }

    /// Drop a seat's reply queue, and only if generation `attach_id` is still live.
    pub(super) fn detach(&self, seat: &str, attach_id: u64) {
        if let Ok(mut routes) = self.routes.lock()
            && routes.get(seat).is_some_and(|q| q.attach_id == attach_id)
        {
            routes.remove(seat);
        }
    }
}
