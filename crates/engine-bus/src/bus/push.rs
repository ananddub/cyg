use std::sync::atomic::Ordering;

use crate::messages::{
    InputMsg, MatchFinished, RoomOwnership, RoomSnapshot, SeatUpdate, SimEvent, StatWrite,
};

use super::*;

impl Bus {
    pub fn request_drain(&self) {
        self.draining.store(true, Ordering::Relaxed);
    }

    pub fn is_draining(&self) -> bool {
        self.draining.load(Ordering::Relaxed)
    }

    pub fn push_input(&self, msg: InputMsg) {
        let mut msg = msg;
        if self.inputs_tx.try_send(msg.clone()).is_ok() {
            return;
        }
        // Drop-oldest until the incoming message fits; every evicted input is
        // counted and the caller's message itself is never dropped (only when
        // the channel is disconnected, where nothing would be delivered).
        loop {
            let _ = self.inputs_rx.try_recv();
            self.inputs_dropped.fetch_add(1, Ordering::Relaxed);
            match self.inputs_tx.try_send(msg) {
                Ok(()) => return,
                Err(crossbeam_channel::TrySendError::Full(m)) => msg = m,
                Err(_) => return,
            }
        }
    }

    pub fn inputs_dropped(&self) -> u64 {
        self.inputs_dropped.load(Ordering::Relaxed)
    }

    pub fn push_event(&self, event: SimEvent) {
        let event = match self.events_tx.try_send(event) {
            Ok(()) => return,
            Err(crossbeam_channel::TrySendError::Full(e)) => e,
            Err(_) => return,
        };
        let _ = self.events_rx.try_recv();
        self.events_dropped.fetch_add(1, Ordering::Relaxed);
        let _ = self.events_tx.try_send(event);
    }

    pub fn events_dropped(&self) -> u64 {
        self.events_dropped.load(Ordering::Relaxed)
    }

    pub fn push_match(&self, match_: MatchFinished) {
        let match_ = match self.matches_tx.try_send(match_) {
            Ok(()) => return,
            Err(crossbeam_channel::TrySendError::Full(m)) => m,
            Err(_) => return,
        };
        let _ = self.matches_rx.try_recv();
        self.matches_dropped.fetch_add(1, Ordering::Relaxed);
        let _ = self.matches_tx.try_send(match_);
    }

    pub fn matches_dropped(&self) -> u64 {
        self.matches_dropped.load(Ordering::Relaxed)
    }

    pub fn push_snapshot(&self, snapshot: RoomSnapshot) {
        let snapshot = match self.snapshots_tx.try_send(snapshot) {
            Ok(()) => return,
            Err(crossbeam_channel::TrySendError::Full(s)) => s,
            Err(_) => return,
        };
        let _ = self.snapshots_rx.try_recv();
        let _ = self.snapshots_tx.try_send(snapshot);
    }

    pub fn push_ownership(&self, row: RoomOwnership) {
        let row = match self.ownership_tx.try_send(row) {
            Ok(()) => return,
            Err(crossbeam_channel::TrySendError::Full(r)) => r,
            Err(_) => return,
        };
        let _ = self.ownership_rx.try_recv();
        self.ownership_dropped.fetch_add(1, Ordering::Relaxed);
        let _ = self.ownership_tx.try_send(row);
    }

    pub fn ownership_dropped(&self) -> u64 {
        self.ownership_dropped.load(Ordering::Relaxed)
    }

    pub fn push_seat(&self, seat: SeatUpdate) {
        let seat = match self.seats_tx.try_send(seat) {
            Ok(()) => return,
            Err(crossbeam_channel::TrySendError::Full(s)) => s,
            Err(_) => return,
        };
        let _ = self.seats_rx.try_recv();
        self.seats_dropped.fetch_add(1, Ordering::Relaxed);
        let _ = self.seats_tx.try_send(seat);
    }

    pub fn seats_dropped(&self) -> u64 {
        self.seats_dropped.load(Ordering::Relaxed)
    }

    /// Queues one validated stat increment. The queue is bounded; when it is
    /// full the oldest intent is evicted and counted, never the simulation
    /// thread that produced it.
    pub fn push_stat(&self, write: StatWrite) {
        let write = match self.stats_tx.try_send(write) {
            Ok(()) => return,
            Err(crossbeam_channel::TrySendError::Full(w)) => w,
            Err(_) => return,
        };
        let _ = self.stats_rx.try_recv();
        self.stats_dropped.fetch_add(1, Ordering::Relaxed);
        let _ = self.stats_tx.try_send(write);
    }

    pub fn stats_dropped(&self) -> u64 {
        self.stats_dropped.load(Ordering::Relaxed)
    }
}
