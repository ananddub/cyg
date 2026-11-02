use std::time::Duration;

use crate::error::SidecarError;

#[derive(Debug, Clone)]
pub struct PingOutcome {
    pub ok: bool,
    pub latency: Duration,
    pub consecutive_failures: u32,
    pub error: Option<SidecarError>,
    pub just_missed_the_window: bool,
}

impl PingOutcome {
    pub fn error_kind(&self) -> Option<&'static str> {
        self.error.as_ref().map(|e| e.kind().as_str())
    }

    pub(crate) fn unavailable() -> PingOutcome {
        PingOutcome {
            ok: false,
            latency: Duration::ZERO,
            consecutive_failures: 0,
            error: Some(SidecarError::Malformed {
                reason: "the health beat's own lock was poisoned".into(),
            }),
            just_missed_the_window: false,
        }
    }
}
