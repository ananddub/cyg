//! Message flow through the hub: publish to browsers, post to the data plane.

use std::sync::atomic::Ordering::Relaxed;

use tokio::sync::mpsc::error::TrySendError;

use super::frames::{SignalIn, SignalOut};
use super::hub::SignalHub;

impl SignalHub {
    /// Route one reply. `false` means it was dropped and counted: no socket is listening
    /// for that seat, its queue is full, or its socket vanished mid-publish.
    pub fn publish(&self, seat: &str, msg: SignalOut) -> bool {
        let Some(tx) = self
            .routes
            .lock()
            .ok()
            .and_then(|r| r.get(seat).map(|q| q.tx.clone()))
        else {
            self.dropped_full.fetch_add(1, Relaxed);
            return false;
        };
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

    /// Queue one browser message for the data plane. Never blocks.
    ///
    /// `false` means the queue is full or no data plane has claimed it yet. Refusing to
    /// *accept* while unclaimed lets the socket answer an `error` immediately, and reading
    /// the hand-over slot is not a claim, so a data plane starting later still finds it.
    pub fn post(&self, msg: SignalIn) -> bool {
        if self.inbound_unowned() {
            self.dropped_full.fetch_add(1, Relaxed);
            return false;
        }
        self.inbound_tx.try_send(msg).is_ok()
    }

    /// True while [`SignalHub::take_signals`] has never been called. `Close` is the
    /// one message hub must attempt even then: only an attempt tells "the data plane
    /// started and this socket went away" from "no data plane ever started".
    fn inbound_unowned(&self) -> bool {
        self.inbound_rx
            .lock()
            .map(|slot| slot.is_some())
            .unwrap_or(false)
    }

    /// Attempt a `Close` regardless of who owns the queue: a notice dropped into a
    /// queue nobody reads is the correct outcome — nothing is waiting for the seat.
    pub(super) fn try_post(&self, msg: SignalIn) -> bool {
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
}
