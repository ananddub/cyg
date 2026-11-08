use std::sync::atomic::Ordering;

use crate::messages::{InputMsg, MatchFinished, RoomOwnership, RoomSnapshot, SeatUpdate, SimEvent};

use super::*;

impl Bus {
    pub fn request_drain(&self) {
        self.draining.store(true, Ordering::Relaxed);
    }

    pub fn is_draining(&self) -> bool {
        self.draining.load(Ordering::Relaxed)
    }

    pub fn push_input(&self, msg: InputMsg) {
        let msg = match self.inputs_tx.try_send(msg) {
            Ok(()) => return,
            Err(crossbeam_channel::TrySendError::Full(m)) => m,
            Err(_) => return,
        };
        let _ = self.inputs_rx.try_recv();
        let _ = self.inputs_tx.try_send(msg);
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
}
