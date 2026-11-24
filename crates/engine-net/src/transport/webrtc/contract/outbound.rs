use std::collections::VecDeque;
use std::sync::Arc;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutboundDelta {
    pub seq: Option<u16>,
    pub tick: Option<u32>,
    pub frames: VecDeque<Arc<[u8]>>,
}

impl OutboundDelta {
    pub fn frames(&self) -> usize {
        self.frames.len()
    }
}
