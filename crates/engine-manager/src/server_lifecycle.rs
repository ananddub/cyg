use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use crate::threads::{ThreadRegistry, ThreadRole, ThreadState, ThreadStatus};

/// Re-exported so `engine-manager` remains the single import site for lifecycle
/// state, while the enum itself lives in `engine-bus` next to [`RoomPhase`] so
/// the status snapshot can serialize it without a dependency cycle.
pub use engine_bus::ServerPhase;

/// Server drain lifecycle state.
///
/// Tracks graceful shutdown progress so the host loop can coordinate draining
/// rooms/clients before exit from a single place.
#[derive(Debug, Clone)]
pub struct DrainState {
    active: bool,
    deadline: Option<Instant>,
    grace: Duration,
}

impl DrainState {
    #[inline]
    pub fn new(grace: Duration) -> Self {
        Self {
            active: false,
            deadline: None,
            grace,
        }
    }

    /// Arm drain mode. Returns `true` only on the first transition to active.
    #[inline]
    pub fn begin(&mut self, now: Instant) -> bool {
        if !self.active {
            self.active = true;
        }
        if self.deadline.is_some() {
            return false;
        }
        self.deadline = Some(now + self.grace);
        true
    }

    #[inline]
    pub fn activate(&mut self) {
        self.active = true;
    }

    #[inline]
    pub fn deactivate(&mut self) {
        self.active = false;
    }

    #[inline]
    pub fn is_active(&self) -> bool {
        self.active
    }

    #[inline]
    pub fn expired(&self, now: Instant) -> bool {
        self.deadline.is_some_and(|at| now >= at)
    }

    #[inline]
    pub fn deadline(&self) -> Option<Instant> {
        self.deadline
    }

    #[inline]
    pub fn grace(&self) -> Duration {
        self.grace
    }
}

impl Default for DrainState {
    fn default() -> Self {
        Self::new(Duration::from_secs(30))
    }
}

/// Central authoritative lifecycle state for the server process/host itself.
///
/// Owns the boot → ready → allocated → draining → shutdown phase machine, the
/// drain grace window, and the registry of long-lived process threads. The
/// host loop, boot sequence, control plane, and signal handler drive this
/// instead of each keeping their own ad-hoc flags.
#[derive(Debug)]
pub struct ServerLifecycle {
    phase: ServerPhase,
    born: Instant,
    drain: DrainState,
    threads: ThreadRegistry,
    /// How long shutdown waits for managed threads to observe the stop flag
    /// before detaching them. A stalled thread must not hold the process open.
    thread_join_grace: Duration,
}

impl Default for ServerLifecycle {
    fn default() -> Self {
        Self::new(Duration::from_secs(30))
    }
}

impl ServerLifecycle {
    /// Creates the server lifecycle in the [`ServerPhase::Booting`] phase.
    pub fn new(grace: Duration) -> Self {
        Self {
            phase: ServerPhase::Booting,
            born: Instant::now(),
            drain: DrainState::new(grace),
            threads: ThreadRegistry::new(),
            thread_join_grace: Duration::from_secs(5),
        }
    }

    #[inline]
    pub fn phase(&self) -> ServerPhase {
        self.phase
    }

    #[inline]
    pub fn born(&self) -> Instant {
        self.born
    }

    #[inline]
    pub fn uptime(&self) -> Duration {
        self.born.elapsed()
    }

    #[inline]
    pub fn is_serving(&self) -> bool {
        self.phase.is_serving()
    }

    #[inline]
    pub fn drain(&self) -> &DrainState {
        &self.drain
    }

    #[inline]
    pub fn drain_mut(&mut self) -> &mut DrainState {
        &mut self.drain
    }

    #[inline]
    pub fn grace(&self) -> Duration {
        self.drain.grace()
    }

    #[inline]
    pub fn threads(&self) -> &ThreadRegistry {
        &self.threads
    }

    #[inline]
    pub fn thread_join_grace(&self) -> Duration {
        self.thread_join_grace
    }

    #[inline]
    pub fn set_thread_join_grace(&mut self, grace: Duration) {
        self.thread_join_grace = grace;
    }

    #[inline]
    pub fn threads_mut(&mut self) -> &mut ThreadRegistry {
        &mut self.threads
    }

    /// Register the calling thread as a managed process thread.
    pub fn register_thread(&mut self, role: ThreadRole, name: &str) {
        self.threads.register(role, name);
    }

    /// Take ownership of an already-spawned process thread.
    pub fn adopt_thread(&mut self, role: ThreadRole, name: &str, handle: JoinHandle<()>) {
        self.threads.adopt(role, name, handle);
    }

    /// Spawn and adopt a named process thread.
    pub fn spawn_thread<F>(&mut self, role: ThreadRole, name: &str, f: F) -> std::io::Result<()>
    where
        F: FnOnce() + Send + 'static,
    {
        self.threads.spawn(role, name, f)
    }

    /// Mark an externally-owned thread's state (e.g. a registered thread with
    /// no join handle that stopped itself).
    pub fn mark_thread(&mut self, name: &str, state: ThreadState) -> bool {
        self.threads.mark(name, state)
    }

    #[inline]
    pub fn request_stop(&mut self) {
        self.threads.request_stop();
    }

    #[inline]
    pub fn stop_requested(&self) -> bool {
        self.threads.stop_requested()
    }

    #[inline]
    pub fn stop_flag(&self) -> Arc<AtomicBool> {
        self.threads.stop_flag()
    }

    #[inline]
    pub fn thread_statuses(&self) -> Vec<ThreadStatus> {
        self.threads.statuses()
    }

    /// Move to `next` if the transition is legal. Returns `true` only when the
    /// phase actually changed.
    pub fn transition(&mut self, next: ServerPhase) -> bool {
        if self.phase == next {
            return false;
        }
        if !self.phase.allows(next) {
            tracing::warn!("[manager] server refused {:?} -> {next:?}", self.phase);
            return false;
        }
        self.phase = next;
        true
    }

    /// Boot finished: the process is ready to serve players.
    #[inline]
    pub fn mark_ready(&mut self) -> bool {
        self.transition(ServerPhase::Ready)
    }

    /// The cluster allocated this server a game.
    #[inline]
    pub fn mark_allocated(&mut self) -> bool {
        self.transition(ServerPhase::Allocated)
    }

    /// A drain was requested. Arms the grace window on the first call and
    /// moves the phase to [`ServerPhase::Draining`]; later calls are no-ops
    /// and return `false`.
    pub fn begin_drain(&mut self, now: Instant) -> bool {
        let first = self.drain.begin(now);
        if first {
            self.transition(ServerPhase::Draining);
        }
        first
    }

    #[inline]
    pub fn is_draining(&self) -> bool {
        self.phase == ServerPhase::Draining
    }

    #[inline]
    pub fn drain_expired(&self, now: Instant) -> bool {
        self.drain.expired(now)
    }

    /// Drain finished: everything is closed, the process should exit.
    #[inline]
    pub fn finish(&mut self) -> bool {
        self.transition(ServerPhase::Shutdown)
    }

    #[inline]
    pub fn is_shutdown(&self) -> bool {
        self.phase == ServerPhase::Shutdown
    }
}

/// Thread-safe handle to the process lifecycle, shared by the host loop and the
/// control plane.
///
/// The host loop is the only hot-path user, so the lock is uncontended there;
/// the control plane only takes it at boot and shutdown.
#[derive(Debug)]
pub struct ServerManager {
    inner: Mutex<ServerLifecycle>,
}

impl ServerManager {
    /// Build a shared manager for a process that must drain within `grace`.
    pub fn new(grace: Duration) -> Arc<Self> {
        Arc::new(Self {
            inner: Mutex::new(ServerLifecycle::new(grace)),
        })
    }

    /// Lock the underlying lifecycle. Callers that need several reads/writes
    /// in one step should hold this instead of taking the lock per call.
    pub fn lock(&self) -> MutexGuard<'_, ServerLifecycle> {
        self.inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    #[inline]
    pub fn phase(&self) -> ServerPhase {
        self.lock().phase()
    }

    #[inline]
    pub fn grace(&self) -> Duration {
        self.lock().grace()
    }

    #[inline]
    pub fn uptime(&self) -> Duration {
        self.lock().uptime()
    }

    #[inline]
    pub fn is_serving(&self) -> bool {
        self.lock().is_serving()
    }

    #[inline]
    pub fn is_draining(&self) -> bool {
        self.lock().is_draining()
    }

    #[inline]
    pub fn is_shutdown(&self) -> bool {
        self.lock().is_shutdown()
    }

    #[inline]
    pub fn mark_ready(&self) -> bool {
        self.lock().mark_ready()
    }

    #[inline]
    pub fn mark_allocated(&self) -> bool {
        self.lock().mark_allocated()
    }

    #[inline]
    pub fn begin_drain(&self, now: Instant) -> bool {
        self.lock().begin_drain(now)
    }

    #[inline]
    pub fn drain_expired(&self, now: Instant) -> bool {
        self.lock().drain_expired(now)
    }

    #[inline]
    pub fn finish(&self) -> bool {
        self.lock().finish()
    }

    pub fn register_thread(&self, role: ThreadRole, name: &str) {
        self.lock().register_thread(role, name);
    }

    pub fn adopt_thread(&self, role: ThreadRole, name: &str, handle: JoinHandle<()>) {
        self.lock().adopt_thread(role, name, handle);
    }

    pub fn spawn_thread<F>(&self, role: ThreadRole, name: &str, f: F) -> std::io::Result<()>
    where
        F: FnOnce() + Send + 'static,
    {
        self.lock().spawn_thread(role, name, f)
    }

    pub fn on_stop<F>(&self, hook: F)
    where
        F: Fn() + Send + 'static,
    {
        self.lock().threads_mut().on_stop(hook);
    }

    pub fn mark_thread(&self, name: &str, state: ThreadState) -> bool {
        self.lock().mark_thread(name, state)
    }

    #[inline]
    pub fn request_stop(&self) {
        self.lock().request_stop();
    }

    #[inline]
    pub fn stop_requested(&self) -> bool {
        self.lock().stop_requested()
    }

    #[inline]
    pub fn stop_flag(&self) -> Arc<AtomicBool> {
        self.lock().stop_flag()
    }

    #[inline]
    pub fn thread_statuses(&self) -> Vec<ThreadStatus> {
        self.lock().thread_statuses()
    }

    /// Request every managed thread to stop and join the adopted ones, but
    /// never block shutdown past the configured join grace.
    pub fn join_threads(&self) {
        let mut lifecycle = self.lock();
        lifecycle.request_stop();
        let grace = lifecycle.thread_join_grace();
        lifecycle.threads_mut().join_all_within(grace);
        tracing::info!("[manager] threads: {}", lifecycle.threads().summary());
    }

    #[inline]
    pub fn set_thread_join_grace(&self, grace: Duration) {
        self.lock().set_thread_join_grace(grace);
    }

    /// Full process shutdown: stop and join threads, then move to `Shutdown`.
    pub fn shutdown(&self) {
        self.join_threads();
        self.finish();
    }

    /// Whether every managed thread has reached a terminal state.
    #[inline]
    pub fn threads_stopped(&self) -> bool {
        self.lock().threads().all_stopped()
    }

    /// Whether the shared stop flag is set (cheap check for tight loops).
    pub fn is_stopping(&self) -> bool {
        self.stop_requested()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::Ordering;

    #[test]
    fn a_fresh_server_starts_booting_and_moves_to_ready_once() {
        let mut s = ServerLifecycle::new(Duration::from_secs(15));
        assert_eq!(s.phase(), ServerPhase::Booting);
        assert!(!s.is_serving());
        assert!(s.mark_ready());
        assert_eq!(s.phase(), ServerPhase::Ready);
        assert!(s.is_serving());
        assert!(!s.mark_ready(), "ready is idempotent");
    }

    #[test]
    fn shutdown_is_terminal() {
        let mut s = ServerLifecycle::new(Duration::from_secs(15));
        s.mark_ready();
        assert!(s.transition(ServerPhase::Shutdown));
        assert!(s.is_shutdown());
        assert!(!s.transition(ServerPhase::Ready));
        assert!(!s.mark_ready());
        assert_eq!(s.phase(), ServerPhase::Shutdown);
    }

    #[test]
    fn illegal_transitions_are_refused() {
        let mut s = ServerLifecycle::new(Duration::from_secs(15));
        // Booting cannot jump straight to Allocated.
        assert!(!s.transition(ServerPhase::Allocated));
        assert_eq!(s.phase(), ServerPhase::Booting);
    }

    #[test]
    fn drain_moves_to_draining_and_arms_the_window_once() {
        let grace = Duration::from_secs(15);
        let mut s = ServerLifecycle::new(grace);
        s.mark_ready();
        let started = Instant::now();

        assert!(!s.drain().expired(started), "nothing requested yet");
        assert!(s.begin_drain(started), "the first pass opens the window");
        assert_eq!(s.phase(), ServerPhase::Draining);
        assert!(s.is_draining());
        assert!(!s.begin_drain(started), "a later pass does not reopen it");
        assert!(!s.drain_expired(started + Duration::from_secs(14)));
        assert!(s.drain_expired(started + grace));
    }

    #[test]
    fn a_drain_from_allocated_is_allowed() {
        let mut s = ServerLifecycle::new(Duration::from_secs(15));
        s.mark_ready();
        assert!(s.mark_allocated());
        assert_eq!(s.phase(), ServerPhase::Allocated);
        assert!(s.begin_drain(Instant::now()));
        assert_eq!(s.phase(), ServerPhase::Draining);
    }

    #[test]
    fn finish_only_advances_to_shutdown() {
        let mut s = ServerLifecycle::new(Duration::from_secs(15));
        s.mark_ready();
        s.begin_drain(Instant::now());
        assert!(s.finish());
        assert!(s.is_shutdown());
        assert!(!s.finish());
    }

    #[test]
    fn the_lifecycle_owns_the_thread_registry() {
        let mut s = ServerLifecycle::new(Duration::from_secs(15));
        s.register_thread(ThreadRole::Host, "cyg-host");
        assert_eq!(s.thread_statuses().len(), 1);
        assert!(s.stop_flag().load(Ordering::Relaxed) == false);
        s.request_stop();
        assert!(s.stop_requested());
        assert_eq!(s.thread_statuses()[0].state, ThreadState::Stopping);
        s.mark_thread("cyg-host", ThreadState::Stopped);
        assert!(s.threads().all_stopped());
    }

    #[test]
    fn a_shared_manager_drives_phase_and_threads() {
        let manager = ServerManager::new(Duration::from_secs(15));
        manager.register_thread(ThreadRole::Host, "cyg-host");
        assert!(manager.mark_ready());
        assert_eq!(manager.phase(), ServerPhase::Ready);
        manager.shutdown();
        assert_eq!(manager.phase(), ServerPhase::Shutdown);
        assert!(manager.stop_requested());
    }

    #[test]
    fn managed_spawned_threads_are_joined_by_the_manager() {
        let manager = ServerManager::new(Duration::from_secs(15));
        let flag = manager.stop_flag();
        manager
            .spawn_thread(ThreadRole::Custom, "worker", move || {
                while !flag.load(Ordering::Relaxed) {
                    std::thread::yield_now();
                }
            })
            .expect("spawn");
        manager.join_threads();
        assert_eq!(
            manager.thread_statuses()[0].state,
            ThreadState::Stopped,
            "a stopped worker is reaped and recorded"
        );
    }
}
