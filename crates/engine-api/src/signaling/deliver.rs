//! Message flow through the hub: publish to browsers, post to the data plane.

use std::sync::atomic::Ordering::Relaxed;

use tokio::sync::mpsc::error::TrySendError;

use super::frames::{SignalIn, SignalOut};
use super::hub::SignalHub;

impl SignalHub {
    /// Route one reply. `false` means the reply was dropped and counted: no
    /// socket is listening for that seat, its queue is full, or its socket
    /// vanished mid-publish — the browser will time out and reconnect.
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
    /// `false` means nobody will read it: the inbound queue is full, or no
    /// data plane has claimed it yet. Refusing to *accept* while the queue is
    /// unclaimed is what lets the socket answer an `error` immediately, and
    /// it is a read of the hand-over slot — not a claim — so a data plane
    /// that starts later still finds the queue waiting.
    pub fn post(&self, msg: SignalIn) -> bool {
        if self.inbound_unowned() {
            self.dropped_full.fetch_add(1, Relaxed);
            return false;
        }
        self.inbound_tx.try_send(msg).is_ok()
    }

    /// True while [`SignalHub::take_signals`] has never been called. `Close`
    /// is the one message hub must attempt even then, because "the data plane
    /// started and this socket went away" and "no data plane ever started"
    /// look identical here, and only an actual attempt tells them apart.
    fn inbound_unowned(&self) -> bool {
        self.inbound_rx.lock().map(|slot| slot.is_some()).unwrap_or(false)
    }

    /// Attempt a `Close` regardless of who owns the queue. A data plane that
    /// never existed drops the notice into a queue nobody reads, which is the
    /// correct outcome — nothing is waiting for the seat.
    pub(super) fn try_post(&self, msg: SignalIn) -> bool {
        self.inbound_tx.try_send(msg).is_ok()
    }

    pub fn peers(&self) -> usize {
        self.routes.lock().map(|r| r.len()).unwrap_or(0)
    }

    pub fn counts(&self) -> (u64, u64) {
        (self.delivered.load(Relaxed), self.dropped_full.load(Relaxed))
    }
}
