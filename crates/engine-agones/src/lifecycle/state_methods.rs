use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use super::{AgonesSnapshot, Lifecycle, poisoned};
use crate::client::{AgonesClient, SidecarConfig};
use crate::error::SidecarError;
use crate::health::{Healthbeat, PingStats};
use crate::state::{ServerState, StateSnapshot};

impl Lifecycle {
    pub fn new(config: SidecarConfig, interval: Duration) -> Arc<Lifecycle> {
        let interval = interval.max(Duration::from_millis(1));
        let client = AgonesClient::new(config);
        let beat = Healthbeat::new(client.clone(), interval);
        let view = AgonesSnapshot {
            enabled: true,
            addr: client.config().addr.clone(),
            interval_ms: interval.as_millis() as u64,
            backoff_ms: beat.current_delay().as_millis() as u64,
            ..AgonesSnapshot::disabled()
        };
        let beat = Mutex::new(beat);
        let view = Mutex::new(view);
        Arc::new(Lifecycle {
            client,
            beat,
            view,
            stop: Arc::new(AtomicBool::new(false)),
            shutdown_sent: AtomicBool::new(false),
            ready_refused: AtomicBool::new(false),
            started: Instant::now(),
            interval,
        })
    }

    pub fn client(&self) -> &AgonesClient {
        &self.client
    }

    pub fn addr(&self) -> &str {
        &self.client.config().addr
    }

    pub fn interval(&self) -> Duration {
        self.interval
    }

    pub fn stats(&self) -> PingStats {
        self.beat.lock().map(|b| b.stats()).unwrap_or_default()
    }

    pub fn snapshot(&self) -> AgonesSnapshot {
        let mut snap = self
            .view
            .lock()
            .map(|v| v.clone())
            .unwrap_or_else(|_| AgonesSnapshot::disabled());
        snap.enabled = true;
        snap.stopped = self.stop.load(Ordering::Relaxed);
        snap.shutdown_sent = self.shutdown_sent.load(Ordering::Acquire);
        snap.uptime_ms = self.started.elapsed().as_millis() as u64;
        if let Ok(beat) = self.beat.lock() {
            snap.pings = beat.stats();
            snap.backoff_ms = beat.current_delay().as_millis() as u64;
        }
        snap
    }

    pub fn mark_ready(&self) -> Result<(), SidecarError> {
        self.transition(
            "ready",
            self.client.post("/ready").map(|_| ()),
            ServerState::Ready,
        )
    }

    pub fn mark_allocated(&self) -> Result<(), SidecarError> {
        self.transition(
            "allocated",
            self.client.post("/allocated").map(|_| ()),
            ServerState::Allocated,
        )
    }

    pub fn refresh_state(&self) -> Result<StateSnapshot, SidecarError> {
        let snapshot = self.client.get_state()?;
        match self.view.lock() {
            Ok(mut view) => {
                view.last_transition = "state".into();
                view.state = snapshot.state;
                view.state_raw = snapshot.state_raw.clone();
                view.last_error = None;
            }
            Err(_) => return Err(poisoned()),
        }
        Ok(snapshot)
    }

    pub fn shutdown(&self) -> Result<bool, SidecarError> {
        if self.shutdown_sent.load(Ordering::Acquire) {
            self.stop();
            return Ok(false);
        }

        if self
            .shutdown_sent
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            self.stop();
            return Ok(false);
        }
        match self.client.post("/shutdown").map(|_| ()) {
            Ok(()) => {
                self.stop();
                if let Ok(mut view) = self.view.lock() {
                    view.last_transition = "shutdown".into();
                    view.state = ServerState::Shutdown;
                    view.state_raw = ServerState::Shutdown.as_str().into();
                    view.last_error = None;
                }
                Ok(true)
            }
            Err(e) => {
                self.shutdown_sent.store(false, Ordering::Release);
                self.record_failure("shutdown", &e);
                Err(e)
            }
        }
    }
}
