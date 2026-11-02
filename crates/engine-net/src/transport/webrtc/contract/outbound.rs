
use std::collections::VecDeque;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutboundDelta {

    pub seq: Option<u16>,
    pub tick: Option<u32>,
    pub frames: VecDeque<Vec<u8>>,
}

impl OutboundDelta {
    pub fn frames(&self) -> usize {
        self.frames.len()
    }
}
