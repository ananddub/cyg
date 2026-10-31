use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

use tokio::sync::mpsc::{Receiver, Sender};

use super::consts::{INBOUND_QUEUE, mpsc_channel};
use super::frames::{SignalIn, SignalOut};
use super::replies::SeatReplies;

pub(super) struct RoutedQueue {
    pub(super) attach_id: u64,
    pub(super) tx: Sender<SignalOut>,
}

#[derive(Clone)]
pub struct SignalHub {
    pub(super) inbound_tx: Sender<SignalIn>,
    pub(super) inbound_rx: Arc<std::sync::Mutex<Option<Receiver<SignalIn>>>>,
    pub(super) routes: Arc<std::sync::Mutex<HashMap<String, RoutedQueue>>>,
    ice_config: Arc<String>,
    pub(super) next_attach: Arc<AtomicU64>,
    pub(super) delivered: Arc<AtomicU64>,
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

    pub fn take_signals(&self) -> Option<Receiver<SignalIn>> {
        self.inbound_rx.lock().ok()?.take()
    }

    pub fn attach(&self, seat: &str) -> SeatReplies {
        let (tx, rx) = mpsc_channel(super::consts::PER_SEAT_QUEUE);
        let attach_id = self
            .next_attach
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
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

    pub(super) fn detach(&self, seat: &str, attach_id: u64) {
        if let Ok(mut routes) = self.routes.lock()
            && routes.get(seat).is_some_and(|q| q.attach_id == attach_id)
        {
            routes.remove(seat);
        }
    }
}
