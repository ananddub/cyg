
use smallvec::SmallVec;
use std::collections::VecDeque;

pub type InputData = SmallVec<[u8; 32]>;

#[derive(Debug, Clone, PartialEq)]
pub struct PlayerInput {

    pub seq: u64,

    pub client_time_ms: f64,

    pub data: InputData,
}

impl PlayerInput {

    pub fn new(seq: u64, client_time_ms: f64, data: impl AsRef<[u8]>) -> Self {
        Self {
            seq,
            client_time_ms,
            data: InputData::from_slice(data.as_ref()),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct PlayerInputBuffer {
    queue: VecDeque<PlayerInput>,

    last_consumed_seq: u64,

    consumed_count: u64,

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

    #[inline]
    pub fn push(&mut self, input: PlayerInput) -> bool {

        if self.consumed_count > 0 && input.seq <= self.last_consumed_seq {
            return false;
        }

        let mut insert_idx = self.queue.partition_point(|i| i.seq < input.seq);

        if insert_idx < self.queue.len() && self.queue[insert_idx].seq == input.seq {
            return false;
        }

        if self.queue.len() >= self.max_backlog {
            if input.seq < self.queue.front().expect("queue at max_backlog is non-empty").seq {
                return false;
            }
            self.queue.pop_front();

            insert_idx = insert_idx.saturating_sub(1);
        }

        self.queue.insert(insert_idx, input);
        true
    }

    #[inline]
    pub fn consume(&mut self) -> Option<PlayerInput> {
        let input = self.queue.pop_front()?;
        self.last_consumed_seq = input.seq;
        self.consumed_count += 1;
        Some(input)
    }

    pub fn consume_batch(&mut self, max_count: usize) -> Vec<PlayerInput> {
        let count = max_count.min(self.queue.len());
        let mut batch = Vec::with_capacity(count);
        self.consume_batch_into(max_count, &mut batch);
        batch
    }

    pub fn consume_batch_into(&mut self, max_count: usize, out: &mut Vec<PlayerInput>) {
        out.clear();
        let count = max_count.min(self.queue.len());
        out.reserve(count);
        for _ in 0..count {
            if let Some(input) = self.consume() {
                out.push(input);
            }
        }
    }

    #[inline]
    pub fn peek(&self) -> Option<&PlayerInput> {
        self.queue.front()
    }

    pub fn last_consumed_seq(&self) -> u64 {
        self.last_consumed_seq
    }

    pub fn consumed_count(&self) -> u64 {
        self.consumed_count
    }

    pub fn pending_count(&self) -> usize {
        self.queue.len()
    }

    pub fn clear(&mut self) {
        self.queue.clear();
    }
}
