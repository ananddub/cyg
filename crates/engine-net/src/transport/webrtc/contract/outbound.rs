//! One delta's frames kept together as the unit backpressure discards.

use std::collections::VecDeque;

/// One delta's frames, kept together so backpressure can discard a whole
/// superseded delta instead of leaving the receiver holding its tail.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutboundDelta {
    /// `None` for a raw write that is not a stamped delta.
    pub seq: Option<u16>,
    pub tick: Option<u32>,
    pub frames: VecDeque<Vec<u8>>,
}

impl OutboundDelta {
    pub fn frames(&self) -> usize {
        self.frames.len()
    }
}
