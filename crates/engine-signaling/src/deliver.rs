use std::sync::atomic::Ordering::Relaxed;

use tokio::sync::mpsc::error::TrySendError;

use super::frames::{SignalIn, SignalOut};
use super::hub::SignalHub;

impl SignalHub {
    pub fn publish(&self, seat: &str, msg: SignalOut) -> bool {
        let lock = self.routes.lock().unwrap_or_else(|e| {
            tracing::warn!("signaling routes lock poisoned while publishing to {seat}");
            e.into_inner()
        });
        let Some(tx) = lock.get(seat).map(|q| q.tx.clone()) else {
            self.dropped_no_route.fetch_add(1, Relaxed);
            return false;
        };
        drop(lock);
        match tx.try_send(msg) {
            Ok(()) => {
                self.delivered.fetch_add(1, Relaxed);
                true
            }
            Err(TrySendError::Full(_)) | Err(TrySendError::Closed(_)) => {
                self.dropped_full.fetch_add(1, Relaxed);
                false
            }
        }
    }

    pub fn post(&self, msg: SignalIn) -> bool {
        if self.inbound_unowned() {
            self.dropped_full.fetch_add(1, Relaxed);
            return false;
        }
        self.inbound_tx.try_send(msg).is_ok()
    }

    fn inbound_unowned(&self) -> bool {
        self.inbound_rx
            .lock()
            .map(|slot| slot.is_some())
            .unwrap_or(false)
    }

    pub fn try_post(&self, msg: SignalIn) -> bool {
        self.inbound_tx.try_send(msg).is_ok()
    }

    pub fn peers(&self) -> usize {
        self.routes.lock().map(|r| r.len()).unwrap_or(0)
    }

    pub fn counts(&self) -> (u64, u64) {
        (
            self.delivered.load(Relaxed),
            self.dropped_full.load(Relaxed),
        )
    }

    /// Publishes dropped because the seat had no live route (distinct from a
    /// full per-seat queue).
    pub fn dropped_no_route(&self) -> u64 {
        self.dropped_no_route.load(Relaxed)
    }
}
