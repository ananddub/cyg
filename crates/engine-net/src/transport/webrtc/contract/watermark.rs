use engine_webrtc_protocol::is_newer;

#[derive(Debug, Clone, Copy, Default)]
pub struct SeqWatermark {
    highest: Option<u16>,
    stale: u64,
}

impl SeqWatermark {
    pub fn get(&self) -> Option<u16> {
        self.highest
    }

    pub fn advance(&mut self, seq: u16) -> bool {
        match self.highest {
            Some(last) if !is_newer(last, seq) => {
                self.stale += 1;
                false
            }
            _ => {
                self.highest = Some(seq);
                true
            }
        }
    }

    pub fn is_stale(&self, seq: u16) -> bool {
        self.highest.is_some_and(|last| !is_newer(last, seq))
    }

    pub fn stale(&self) -> u64 {
        self.stale
    }

    pub fn reset(&mut self) {
        *self = SeqWatermark::default();
    }
}
