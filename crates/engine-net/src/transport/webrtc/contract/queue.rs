//! The bounded, drop-oldest send queue one lane of one session holds.

use std::collections::VecDeque;

use engine_webrtc_protocol::is_newer;

use super::outbound::OutboundDelta;

/// Bounded, drop-oldest send queue for one lane of one session.
///
/// `str0m`'s `write` returns `Ok(false)` when its SCTP buffer is full. That is
/// the signal to start holding frames here; once the queue is full, the
/// *oldest* delta leaves and the newest takes its place. A 128 Hz server has no
/// use for a stale world state, and blocking the mio poll loop for one slow
/// client would stall every other session in the thread.
#[derive(Debug, Default)]
pub struct OutboundQueue {
    pub(super) items: VecDeque<OutboundDelta>,
    frames: usize,
    dropped_deltas: u64,
    dropped_frames: u64,
}

impl OutboundQueue {
    /// Queue one delta, discarding older ones to stay within `capacity` frames.
    /// Returns how many deltas and how many frames were discarded.
    pub fn push(&mut self, item: OutboundDelta, capacity: usize) -> (u64, u64) {
        // A delta bigger than the whole queue is its own verdict: counted and refused rather than evicting everything and still not fitting.
        if item.frames() > capacity {
            self.dropped_deltas += 1;
            self.dropped_frames += item.frames() as u64;
            return (1, item.frames() as u64);
        }
        let mut dropped_deltas = 0u64;
        let mut dropped_frames = 0u64;
        while self.frames + item.frames() > capacity {
            let Some(evicted) = self.items.pop_front() else {
                break;
            };
            self.frames -= evicted.frames();
            dropped_deltas += 1;
            dropped_frames += evicted.frames() as u64;
        }
        self.dropped_deltas += dropped_deltas;
        self.dropped_frames += dropped_frames;
        self.frames += item.frames();
        self.items.push_back(item);
        (dropped_deltas, dropped_frames)
    }

    /// Restore a partially written delta to the head after a drain stopped
    /// mid-message, so the lane's order survives the saturation.
    pub fn push_front(&mut self, item: OutboundDelta) {
        self.frames += item.frames();
        self.items.push_front(item);
    }

    /// Discard queued deltas that are no longer newer than `seq`, from the front
    /// of the queue in, and report how many left. Newest wins on the unreliable
    /// lane: a tick that has already been overtaken is worth less than the queue
    /// space it occupies. Raw writes without a `seq` keep their place, since
    /// nothing about them has been superseded.
    pub fn purge_older_than(&mut self, seq: u16) -> usize {
        let mut dropped = 0;
        while let Some(front) = self.items.front() {
            let Some(queued) = front.seq else { break };
            if queued != seq && !is_newer(queued, seq) {
                break;
            }
            let item = self.items.pop_front().expect("front just checked");
            self.frames -= item.frames();
            dropped += 1;
        }
        dropped
    }

    pub fn pop(&mut self) -> Option<OutboundDelta> {
        let item = self.items.pop_front()?;
        self.frames -= item.frames();
        Some(item)
    }

    pub fn peek(&self) -> Option<&OutboundDelta> {
        self.items.front()
    }

    /// Frames the queue is holding right now.
    pub fn frames(&self) -> usize {
        self.frames
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn clear(&mut self) -> usize {
        let count = self.items.iter().map(|d| d.frames()).sum();
        self.items.clear();
        self.frames = 0;
        count
    }

    pub fn dropped_deltas(&self) -> u64 {
        self.dropped_deltas
    }

    pub fn dropped_frames(&self) -> u64 {
        self.dropped_frames
    }
}
