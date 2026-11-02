use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Instant;

use super::{Lifecycle, SidecarErrorInfo, SLEEP_SLICE, Step, poisoned};
use crate::error::{SidecarError, SidecarErrorKind};
use crate::health::PingOutcome;
use crate::state::ServerState;

#[derive(Debug)]
pub enum LifecycleEvent<'a> {
    Ping(&'a PingOutcome),
    ReadyRetried(Option<&'a SidecarError>),
}

impl Lifecycle {
    pub fn stop(&self) {
        self.stop.store(true, Ordering::Release);
    }

    pub fn is_stopped(&self) -> bool {
        self.stop.load(Ordering::Acquire)
    }

    pub fn stop_flag(&self) -> Arc<std::sync::atomic::AtomicBool> {
        Arc::clone(&self.stop)
    }

    pub fn ping_once(&self) -> PingOutcome {
        let outcome = match self.beat.lock() {
            Ok(mut beat) => beat.ping(Instant::now()),
            Err(_) => PingOutcome::unavailable(),
        };
        self.record_ping(&outcome);
        outcome
    }

    pub fn run_until_stopped(&self, mut on_event: impl FnMut(&LifecycleEvent)) {
        while !self.is_stopped() {
            let now = Instant::now();
            let step = match self.beat.lock() {
                Ok(mut beat) => {
                    if beat.is_due(now) {
                        Step::Ping(beat.ping(now))
                    } else {
                        Step::Wait(beat.wait(now))
                    }
                }
                Err(_) => Step::Wait(SLEEP_SLICE),
            };
            match step {
                Step::Ping(outcome) => {
                    self.record_ping(&outcome);
                    let live = outcome.ok;
                    on_event(&LifecycleEvent::Ping(&outcome));
                    if live && !self.ready_accepted() && !self.ready_refused() {
                        let retry = self.mark_ready();
                        if let Err(e) = &retry
                            && e.kind() == SidecarErrorKind::Rejected
                        {
                            self.ready_refused.store(true, Ordering::Release);
                        }
                        on_event(&LifecycleEvent::ReadyRetried(retry.as_ref().err()));
                    }
                }
                Step::Wait(wait) => {
                    if !wait.is_zero() {
                        std::thread::sleep(wait.min(SLEEP_SLICE));
                    }
                }
            }
        }
    }

    pub fn ready_accepted(&self) -> bool {
        self.view.lock().map(|v| v.ready).unwrap_or(false)
    }

    pub fn ready_refused(&self) -> bool {
        self.ready_refused.load(Ordering::Acquire)
    }

    pub(crate) fn transition(
        &self,
        name: &str,
        result: Result<(), SidecarError>,
        becomes: ServerState,
    ) -> Result<(), SidecarError> {
        match result {
            Ok(()) => {
                match self.view.lock() {
                    Ok(mut view) => {
                        view.last_transition = name.into();
                        view.state = becomes;
                        view.state_raw = becomes.as_str().into();
                        view.last_error = None;
                        if becomes == ServerState::Ready {
                            view.ready = true;
                        }
                    }
                    Err(_) => return Err(poisoned()),
                }
                Ok(())
            }
            Err(e) => {
                self.record_failure(name, &e);
                Err(e)
            }
        }
    }

    pub(crate) fn record_failure(&self, transition: &str, error: &SidecarError) {
        if let Ok(mut view) = self.view.lock() {
            view.last_transition = transition.into();
            view.last_error = Some(SidecarErrorInfo::new(error));
        }
    }

    pub(crate) fn record_ping(&self, outcome: &PingOutcome) {
        let Ok(mut view) = self.view.lock() else {
            return;
        };
        match &outcome.error {
            Some(e) => view.last_error = Some(SidecarErrorInfo::new(e)),
            None => {
                view.last_error = None;
            }
        }
    }
}
