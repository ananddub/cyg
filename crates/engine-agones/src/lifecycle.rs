//! The lifecycle orchestrator: one object that owns the client, the beat and the
//! reportable view of both.
//!
//! This is the type the control plane holds. It exists so the pieces above are
//! never wired together differently in two places, and so `/status` can answer
//! "what does this process think about Agones?" without talking to Kubernetes.
//!
//! The global registry ([`install`], [`active`], [`snapshot`]) is the seam for a
//! process that starts the loop on one thread and wants to stop it from another
//! (or from a signal handler it does not yet have).

use std::sync::Arc;
use std::sync::Mutex;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use serde::Serialize;

use crate::client::{AgonesClient, SidecarConfig};
use crate::error::{SidecarError, SidecarErrorKind};
use crate::health::{Healthbeat, PingOutcome, PingStats};
use crate::state::{ServerState, StateSnapshot};

/// Longest single sleep in [`Lifecycle::run_until_stopped`], so `stop` is
/// noticed promptly without spinning.
const SLEEP_SLICE: Duration = Duration::from_millis(20);

/// A sidecar failure reduced to what is safe and useful to put in JSON.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SidecarErrorInfo {
    /// `unreachable` | `timeout` | `rejected` | `malformed`.
    pub kind: String,
    pub message: String,
}

impl SidecarErrorInfo {
    pub fn new(error: &SidecarError) -> SidecarErrorInfo {
        SidecarErrorInfo { kind: error.kind().as_str().to_string(), message: error.message() }
    }
}

/// What this process knows about its own Agones lifecycle. The `/status` payload.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgonesSnapshot {
    /// Is the lifecycle client running at all? `false` means this process never
    /// spoke to a sidecar, which is the normal state outside Kubernetes.
    pub enabled: bool,
    /// `host:port` we dial. Empty when disabled.
    pub addr: String,
    /// Last state we observed or inferred.
    pub state: ServerState,
    /// The sidecar's own spelling of that state, empty until we heard one.
    pub state_raw: String,
    /// Did `/ready` ever get accepted. A pod that was never Ready is never in the
    /// pool, whatever it believes now.
    pub ready: bool,
    /// Last transition we attempted: `ready` | `allocated` | `shutdown` | `state`.
    pub last_transition: String,
    /// The most recent failure, kept until a success replaces it.
    pub last_error: Option<SidecarErrorInfo>,
    /// Has a `/shutdown` been accepted. Idempotency anchor for the stop path.
    pub shutdown_sent: bool,
    /// Has the health loop been asked to stop.
    pub stopped: bool,
    /// Configured `health_interval_ms`.
    pub interval_ms: u64,
    /// The delay the backoff logic is currently using, which grows with the
    /// failure streak: a fast way to tell "just deployed" from "degrading".
    pub backoff_ms: u64,
    /// How long this lifecycle object has existed.
    pub uptime_ms: u64,
    /// Ping statistics: counts, streaks, latency.
    #[serde(flatten)]
    pub pings: PingStats,
}

impl Default for AgonesSnapshot {
    fn default() -> AgonesSnapshot {
        AgonesSnapshot::disabled()
    }
}

impl AgonesSnapshot {
    /// The honest answer when Agones is off: not failing, simply absent.
    pub fn disabled() -> AgonesSnapshot {
        AgonesSnapshot {
            enabled: false,
            addr: String::new(),
            state: ServerState::Unknown,
            state_raw: String::new(),
            ready: false,
            last_transition: String::new(),
            last_error: None,
            shutdown_sent: false,
            stopped: false,
            interval_ms: 0,
            backoff_ms: 0,
            uptime_ms: 0,
            pings: PingStats::default(),
        }
    }
}

/// One iteration of [`Lifecycle::run_until_stopped`]: either a heartbeat happened
/// or we know how long to wait before asking again. Keeps the beat mutex released
/// while the thread sleeps.
enum Step {
    Ping(PingOutcome),
    Wait(Duration),
}

/// What the beat loop has to report, in the order it happened. The caller logs or
/// counts; it never decides whether the loop continues.
#[derive(Debug)]
pub enum LifecycleEvent<'a> {
    /// A heartbeat and its outcome.
    Ping(&'a PingOutcome),
    /// `/ready` was retried after a heartbeat proved the sidecar is alive. `None`
    /// means the retry was accepted and this pod is now in the pool.
    ReadyRetried(Option<&'a SidecarError>),
}

/// Owns one sidecar connection pattern: transitions, the beat, and the reportable
/// view. Share it with `Arc`; every method is `&self` and thread-safe.
#[derive(Debug)]
pub struct Lifecycle {
    client: AgonesClient,
    beat: Mutex<Healthbeat>,
    view: Mutex<AgonesSnapshot>,
    stop: Arc<AtomicBool>,
    shutdown_sent: AtomicBool,
    /// Set once the sidecar has *rejected* `/ready`. Distinct from "never reached
    /// it", which the loop keeps retrying.
    ready_refused: AtomicBool,
    started: Instant,
    interval: Duration,
}

impl Lifecycle {
    /// Build a lifecycle for `config` with the given health interval. The first
    /// heartbeat is due immediately, which is what you want right after `/ready`:
    /// the sidecar only accepts a pod into the pool once it has been Ready.
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

    /// The current reportable view, with `uptime_ms` filled in.
    pub fn snapshot(&self) -> AgonesSnapshot {
        let mut snap = self.view.lock().map(|v| v.clone()).unwrap_or_else(|_| AgonesSnapshot::disabled());
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

    /// `POST /ready`. On success the pod is in the pool and the state becomes
    /// `Ready`; on a rejection the failure is recorded and returned unchanged, so
    /// the caller can tell "no sidecar" from "wrong state".
    pub fn mark_ready(&self) -> Result<(), SidecarError> {
        self.transition("ready", self.client.post("/ready").map(|_| ()), ServerState::Ready)
    }

    /// `POST /allocated` — the manual allocation handoff.
    pub fn mark_allocated(&self) -> Result<(), SidecarError> {
        self.transition("allocated", self.client.post("/allocated").map(|_| ()), ServerState::Allocated)
    }

    /// `GET /state` and remember what it said. Also the way to correct an
    /// inferred state after a rejected transition.
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

    /// `POST /shutdown`, **idempotent**: the first accepted call sends it, every
    /// later call reports `Ok(false)` without touching the socket again, and the
    /// health loop is stopped as part of the same call.
    ///
    /// A failure releases the claim so the operator's stop path can retry — the
    /// common reason for one is that the sidecar is already gone, in which case
    /// there is nothing left to tell.
    ///
    /// Returns `Ok(true)` if this call sent it, `Ok(false)` if it was already sent
    /// or is in flight on another thread.
    pub fn shutdown(&self) -> Result<bool, SidecarError> {
        if self.shutdown_sent.load(Ordering::Acquire) {
            self.stop();
            return Ok(false);
        }
        // Claim the send before doing it, so two stop paths cannot both POST.
        if self.shutdown_sent.compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire).is_err() {
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
                // Release the claim: the transition did not happen.
                self.shutdown_sent.store(false, Ordering::Release);
                self.record_failure("shutdown", &e);
                Err(e)
            }
        }
    }

    /// Ask the health loop to stop. Idempotent, and the only way to end
    /// [`run_until_stopped`] from another thread.
    pub fn stop(&self) {
        self.stop.store(true, Ordering::Release);
    }

    pub fn is_stopped(&self) -> bool {
        self.stop.load(Ordering::Acquire)
    }

    /// The flag itself, for a thread that wants to watch it.
    pub fn stop_flag(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.stop)
    }

    /// One heartbeat now, counted and recorded. Usable by a caller that owns its
    /// own timing (and by tests that do not want a thread).
    pub fn ping_once(&self) -> PingOutcome {
        let outcome = match self.beat.lock() {
            Ok(mut beat) => beat.ping(Instant::now()),
            Err(_) => PingOutcome::unavailable(),
        };
        self.record_ping(&outcome);
        outcome
    }

    /// The blocking beat loop for one dedicated OS thread. Records every outcome
    /// into the snapshot and reports each interesting thing to `on_event`
    /// (logging, metrics) as it happens.
    ///
    /// It also settles the start-of-pod race: if `/ready` has not been accepted
    /// yet, the first heartbeat that reaches the sidecar retries it, because a
    /// sidecar container that was not up when we first knocked is normal and a pod
    /// that never got Ready is never in the allocation pool. A **rejected** retry
    /// stops the retries for good — that is the agent saying this GameServer must
    /// not be Ready, and hammering it is worse than logging it.
    ///
    /// Returns when [`stop`](Self::stop) is set. Never panics on sidecar
    /// behaviour: `run_until_stopped` is the reason a refused connection cannot
    /// take the game server down.
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
                // A poisoned beat mutex means some thread died holding it. The pod
                // still needs its heartbeats, so keep ticking instead of exiting:
                // quitting here would itself make us Unhealthy.
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

    /// Has `/ready` ever been accepted.
    pub fn ready_accepted(&self) -> bool {
        self.view.lock().map(|v| v.ready).unwrap_or(false)
    }

    /// Has `/ready` been refused by the sidecar (as opposed to never reaching it).
    pub fn ready_refused(&self) -> bool {
        self.ready_refused.load(Ordering::Acquire)
    }

    fn transition(
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

    fn record_failure(&self, transition: &str, error: &SidecarError) {
        if let Ok(mut view) = self.view.lock() {
            view.last_transition = transition.into();
            view.last_error = Some(SidecarErrorInfo::new(error));
        }
    }

    fn record_ping(&self, outcome: &PingOutcome) {
        let Ok(mut view) = self.view.lock() else { return };
        match &outcome.error {
            Some(e) => view.last_error = Some(SidecarErrorInfo::new(e)),
            None => {
                // A successful heartbeat proves the sidecar is there, which is the
                // end of the "not deployed yet" story.
                view.last_error = None;
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Process-wide handle
// ---------------------------------------------------------------------------

static ACTIVE: OnceLock<Arc<Lifecycle>> = OnceLock::new();

/// Register the process's lifecycle so a thread that never saw it can still stop
/// it. `Err` returns the argument when one is already installed — the second
/// caller is a bug in the startup path, not something to fail a pod over.
pub fn install(lifecycle: Arc<Lifecycle>) -> Result<(), Arc<Lifecycle>> {
    ACTIVE.set(lifecycle)
}

/// The installed lifecycle, if any.
pub fn active() -> Option<Arc<Lifecycle>> {
    ACTIVE.get().cloned()
}

/// The lifecycle view for `/status`: the live snapshot when one is installed, the
/// honest "not deployed" default when it is not.
pub fn snapshot() -> AgonesSnapshot {
    match ACTIVE.get() {
        Some(lifecycle) => lifecycle.snapshot(),
        None => AgonesSnapshot::disabled(),
    }
}

/// The operator stop path: stop the beat and tell the sidecar to retire this pod,
/// once. `Ok(true)` we sent it, `Ok(false)` it was already sent, `Err` either no
/// lifecycle is installed or the sidecar refused/could not be reached — with the
/// message to log.
pub fn shutdown_sidecar() -> Result<bool, String> {
    let Some(lifecycle) = active() else {
        return Err("no Agones lifecycle is running in this process".to_string());
    };
    // Stop the loop first: after a stop request, more heartbeats are noise.
    lifecycle.stop();
    lifecycle.shutdown().map_err(|e| e.message())
}

/// Stop the beat loop without asserting a shutdown transition. Returns `false`
/// when nothing is installed.
pub fn stop_health_loop() -> bool {
    match active() {
        Some(lifecycle) => {
            lifecycle.stop();
            true
        }
        None => false,
    }
}

fn poisoned() -> SidecarError {
    SidecarError::Malformed { reason: "the lifecycle's own state lock was poisoned".into() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disabled_snapshot_says_so() {
        let json = serde_json::to_string(&AgonesSnapshot::disabled()).unwrap();
        assert!(json.contains("\"enabled\":false"), "{json}");
        assert!(json.contains("\"state\":\"Unknown\""), "{json}");
        assert!(json.contains("\"consecutiveFailures\":0"), "{json}");
    }

    #[test]
    fn snapshot_flattens_ping_stats() {
        let snap = Lifecycle::new(
            SidecarConfig::loopback(9358),
            Duration::from_millis(2000),
        )
        .snapshot();
        let json = serde_json::to_string(&snap).unwrap();
        assert!(json.contains("\"addr\":\"127.0.0.1:9358\""), "{json}");
        assert!(json.contains("\"enabled\":true"), "{json}");
        assert!(json.contains("\"intervalMs\":2000"), "{json}");
        // PingStats is flattened into the object, not nested under "pings".
        assert!(json.contains("\"pings\":0"), "{json}");
        assert!(!json.contains("\"pings\":{"), "{json}");
    }
}
