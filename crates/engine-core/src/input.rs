//! # Client Input Buffering & Reconciliation
//!
//! Handles client input sequencing, deduplication, and reconciliation acknowledgments.
//! The server tracks how many inputs have been consumed (`consumed_count`), which is
//! sent back to the client so the client knows which predicted inputs have been confirmed.

use std::collections::VecDeque;

/// An input packet received from a client.
#[derive(Debug, Clone, PartialEq)]
pub struct PlayerInput {
    /// Monotonically increasing sequence number assigned by client.
    pub seq: u64,
    /// Client-side timestamp when this input was sampled (for lag-compensation timeline matching).
    pub client_time_ms: f64,
    /// Raw input payload (movement delta, button mask, aim angles).
    pub data: Vec<u8>,
}

impl PlayerInput {
    pub fn new(seq: u64, client_time_ms: f64, data: Vec<u8>) -> Self {
        Self {
            seq,
            client_time_ms,
            data,
        }
    }
}

/// Ordered input queue for a single connected player.
#[derive(Debug, Clone, Default)]
pub struct PlayerInputBuffer {
    queue: VecDeque<PlayerInput>,
    /// Last sequence number consumed by the server simulation.
    last_consumed_seq: u64,
    /// Total number of inputs consumed so far.
    /// This is the canonical **Reconciliation Acknowledgment** echoed back to the client!
    consumed_count: u64,
    /// Maximum backlog before dropping oldest unconsumed inputs (prevents memory explosion from stalled clients).
    max_backlog: usize,
}

impl PlayerInputBuffer {
    pub fn new(max_backlog: usize) -> Self {
        Self {
            queue: VecDeque::with_capacity(max_backlog.min(128)),
            last_consumed_seq: 0,
            consumed_count: 0,
            max_backlog: max_backlog.max(16),
        }
    }

    /// Push a new input into the buffer.
    /// Deduplicates already-consumed inputs and keeps queue strictly sorted by sequence number.
    /// O(log n): binary search locates the insert position and detects duplicates.
    pub fn push(&mut self, input: PlayerInput) -> bool {
        // Discard if already consumed by a previous tick
        if self.consumed_count > 0 && input.seq <= self.last_consumed_seq {
            return false;
        }

        // Binary search: first element with seq >= input.seq
        let mut insert_idx = self.queue.partition_point(|i| i.seq < input.seq);

        // Duplicate already in queue
        if insert_idx < self.queue.len() && self.queue[insert_idx].seq == input.seq {
            return false;
        }

        // Enforce max backlog by dropping oldest if overflowing
        if self.queue.len() >= self.max_backlog {
            self.queue.pop_front();
            // After dropping the front, our insert position shifts left by one.
            insert_idx = insert_idx.saturating_sub(1);
        }

        self.queue.insert(insert_idx, input);
        true
    }

    /// Consume the oldest pending input.
    /// Advances `consumed_count` and updates `last_consumed_seq`.
    pub fn consume(&mut self) -> Option<PlayerInput> {
        let input = self.queue.pop_front()?;
        self.last_consumed_seq = input.seq;
        self.consumed_count += 1;
        Some(input)
    }

    /// Consume all pending inputs up to `max_count`.
    pub fn consume_batch(&mut self, max_count: usize) -> Vec<PlayerInput> {
        let count = max_count.min(self.queue.len());
        let mut batch = Vec::with_capacity(count);
        for _ in 0..count {
            if let Some(input) = self.consume() {
                batch.push(input);
            }
        }
        batch
    }

    /// Peek at the next input without consuming it.
    pub fn peek(&self) -> Option<&PlayerInput> {
        self.queue.front()
    }

    /// Sequence number of the last consumed input.
    pub fn last_consumed_seq(&self) -> u64 {
        self.last_consumed_seq
    }

    /// Cumulative count of inputs consumed so far.
    /// This is the reconciliation ack sent to the client SDK!
    pub fn consumed_count(&self) -> u64 {
        self.consumed_count
    }

    /// Number of inputs currently queued and awaiting simulation.
    pub fn pending_count(&self) -> usize {
        self.queue.len()
    }

    /// Clear all pending inputs.
    pub fn clear(&mut self) {
        self.queue.clear();
    }
}
