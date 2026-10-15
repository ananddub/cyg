use engine_agones::{PingOutcome, UNHEALTHY_AFTER_MISSES};

/// Which heartbeat outcomes earn a log line.
///
/// A pod that cannot reach its sidecar logs every interval for minutes on
/// end, and a log that never changes is a log nobody reads. So: the first
/// [`UNHEALTHY_AFTER_MISSES`] misses, then only the crossing of it and every
/// doubling of the streak — plus exactly one line when the streak breaks.
#[derive(Debug, Default)]
pub(super) struct Loudness {
    /// Misses already logged for the current streak; `0` while healthy.
    logged: u32,
}

impl Loudness {
    pub(super) fn should_log(&mut self, outcome: &PingOutcome) -> bool {
        if outcome.ok {
            // Report the recovery only if we ever reported the failure.
            let was_loud = self.logged > 0;
            self.logged = 0;
            return was_loud;
        }
        let misses = outcome.consecutive_failures;
        let noteworthy = outcome.just_missed_the_window
            || misses <= UNHEALTHY_AFTER_MISSES
            || misses.is_power_of_two();
        if noteworthy {
            self.logged = misses;
        }
        noteworthy
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn miss(n: u32) -> PingOutcome {
        PingOutcome {
            ok: false,
            latency: Duration::from_millis(1),
            consecutive_failures: n,
            error: Some(engine_agones::SidecarError::Unreachable {
                addr: "127.0.0.1:9358".into(),
                reason: "Connection refused (os error 111)".into(),
            }),
            just_missed_the_window: n == UNHEALTHY_AFTER_MISSES,
        }
    }

    fn beat() -> PingOutcome {
        PingOutcome {
            ok: true,
            latency: Duration::from_millis(2),
            consecutive_failures: 0,
            error: None,
            just_missed_the_window: false,
        }
    }

    #[test]
    fn loudness_reports_the_window_then_goes_quiet() {
        let mut loud = Loudness::default();
        assert!(loud.should_log(&miss(1)));
        assert!(loud.should_log(&miss(2)));
        assert!(loud.should_log(&miss(3)), "the third miss is Agones' unhealthy window");
        assert!(loud.should_log(&miss(4)));
        assert!(!loud.should_log(&miss(5)), "a streak this long must not spam every interval");
        assert!(loud.should_log(&miss(8)), "but every doubling is still news");
        assert!(loud.should_log(&beat()), "and the recovery is exactly one line");
        assert!(!loud.should_log(&beat()), "then nothing while healthy");
    }

    #[test]
    fn a_clean_run_logs_nothing() {
        let mut loud = Loudness::default();
        for _ in 0..5 {
            assert!(!loud.should_log(&beat()), "a healthy pod has no reason to print");
        }
    }
}
