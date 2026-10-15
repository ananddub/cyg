//! The forward-only sequence high-water mark.

use engine_webrtc_protocol::is_newer;

/// Forward-only high-water mark over a wrapping `u16` counter.
///
/// Same rule the delta assembler uses: `is_newer` is wrap-safe, so a client
/// that crosses the 65535 boundary does not look like it restarted. Duplicates
/// and late frames are counted, never applied — except that a stale *input* is
/// still reported to the host (flagged in `Inbound::Input::resupplied`),
/// because rollback replay needs genuinely late inputs rather than dropping
/// them.
#[derive(Debug, Clone, Copy, Default)]
pub struct SeqWatermark {
    highest: Option<u16>,
    stale: u64,
}

impl SeqWatermark {
    pub fn get(&self) -> Option<u16> {
        self.highest
    }

    /// `true` if `seq` was newer and is now the mark. `false` = stale, counted.
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

    /// `true` if `seq` would not move the mark.
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
