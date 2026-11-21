//! Thread supervision for the game-server process.
//!
//! The lifecycle answers *what phase* the process is in; this registry answers
//! *which threads are running for it*. Together they let one authority start,
//! observe, stop, and join the host loop and its long-lived helpers instead of
//! each component owning an untracked `JoinHandle` that dies with the process.

use std::fmt;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

/// What a supervised thread is for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ThreadRole {
    /// The main simulation/host loop.
    Host,
    /// The Tokio control plane (HTTP/matchmake/signaling workers).
    Control,
    /// The optional pinned UDP sim plane (`CYG_PLANES=1`).
    SimPlane,
    /// The Agones sidecar heartbeat loop.
    Agones,
    /// The WASM epoch interruption ticker.
    WasmEpoch,
    /// The WebRTC driver thread.
    RtcPlane,
    /// Anything else a project registers.
    Custom,
}

impl ThreadRole {
    #[inline]
    pub fn as_str(self) -> &'static str {
        match self {
            ThreadRole::Host => "host",
            ThreadRole::Control => "control",
            ThreadRole::SimPlane => "sim-plane",
            ThreadRole::Agones => "agones",
            ThreadRole::WasmEpoch => "wasm-epoch",
            ThreadRole::RtcPlane => "rtc-plane",
            ThreadRole::Custom => "custom",
        }
    }
}

/// Where a supervised thread is in its own small lifecycle.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ThreadState {
    /// Handle created, not yet marked running.
    Spawned,
    /// Actively working.
    Running,
    /// Stop was requested; the thread has not observed it yet.
    Stopping,
    /// The thread returned (joined cleanly).
    Stopped,
    /// The thread panicked or could not be spawned.
    Failed,
}

impl ThreadState {
    #[inline]
    pub fn as_str(self) -> &'static str {
        match self {
            ThreadState::Spawned => "spawned",
            ThreadState::Running => "running",
            ThreadState::Stopping => "stopping",
            ThreadState::Stopped => "stopped",
            ThreadState::Failed => "failed",
        }
    }

    #[inline]
    pub fn is_terminal(self) -> bool {
        matches!(self, ThreadState::Stopped | ThreadState::Failed)
    }
}

/// A plain-data view of one supervised thread, safe to log or serialize.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ThreadStatus {
    pub role: ThreadRole,
    pub name: String,
    pub state: ThreadState,
}

struct ThreadEntry {
    role: ThreadRole,
    name: String,
    state: ThreadState,
    handle: Option<JoinHandle<()>>,
}

/// Owns the process's long-lived threads and the shared stop flag they poll.
///
/// `register` is for the calling thread (the host main thread, which cannot be
/// joined). `adopt` takes ownership of a `JoinHandle` a component already
/// spawned; `spawn` does both. `request_stop` flips the flag and runs the
/// registered stop hooks, and `join_all` reaps the adopted handles.
pub struct ThreadRegistry {
    stop: Arc<AtomicBool>,
    entries: Vec<ThreadEntry>,
    stop_hooks: Vec<Box<dyn Fn() + Send + 'static>>,
}

impl Default for ThreadRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Debug for ThreadRegistry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ThreadRegistry")
            .field("stopping", &self.stop.load(Ordering::Relaxed))
            .field("entries", &self.statuses())
            .finish()
    }
}

impl ThreadRegistry {
    pub fn new() -> Self {
        Self {
            stop: Arc::new(AtomicBool::new(false)),
            entries: Vec::new(),
            stop_hooks: Vec::new(),
        }
    }

    /// The flag every managed thread should poll. Set once on shutdown.
    #[inline]
    pub fn stop_flag(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.stop)
    }

    #[inline]
    pub fn stop_requested(&self) -> bool {
        self.stop.load(Ordering::Relaxed)
    }

    /// Register the calling thread (no join handle). Starts `Running`.
    pub fn register(&mut self, role: ThreadRole, name: impl Into<String>) -> &mut Self {
        self.entries.push(ThreadEntry {
            role,
            name: name.into(),
            state: ThreadState::Running,
            handle: None,
        });
        self
    }

    /// Take ownership of an already-spawned thread.
    pub fn adopt(&mut self, role: ThreadRole, name: impl Into<String>, handle: JoinHandle<()>) {
        self.entries.push(ThreadEntry {
            role,
            name: name.into(),
            state: ThreadState::Running,
            handle: Some(handle),
        });
    }

    /// Spawn a named thread and adopt it.
    pub fn spawn<F>(
        &mut self,
        role: ThreadRole,
        name: impl Into<String>,
        f: F,
    ) -> std::io::Result<()>
    where
        F: FnOnce() + Send + 'static,
    {
        let name = name.into();
        let handle = std::thread::Builder::new()
            .name(name.clone())
            .spawn(f)?;
        self.adopt(role, name, handle);
        Ok(())
    }

    /// Called by `request_stop` before handles are joined. Use it for threads
    /// that own their own stop mechanism (e.g. the Agones heartbeat loop).
    pub fn on_stop<F>(&mut self, hook: F)
    where
        F: Fn() + Send + 'static,
    {
        self.stop_hooks.push(Box::new(hook));
    }

    /// Change one entry's state. Returns `false` when no such thread exists.
    pub fn mark(&mut self, name: &str, state: ThreadState) -> bool {
        for entry in self.entries.iter_mut().rev() {
            if entry.name == name {
                entry.state = state;
                return true;
            }
        }
        false
    }

    #[inline]
    pub fn state_of(&self, name: &str) -> Option<ThreadState> {
        self.entries
            .iter()
            .rev()
            .find(|entry| entry.name == name)
            .map(|entry| entry.state)
    }

    /// Flip the shared stop flag, mark live entries `Stopping`, and run hooks.
    pub fn request_stop(&mut self) {
        self.stop.store(true, Ordering::Release);
        for entry in &mut self.entries {
            if !entry.state.is_terminal() {
                entry.state = ThreadState::Stopping;
            }
        }
        for hook in &self.stop_hooks {
            hook();
        }
    }

    /// Join every adopted thread with a default grace of 30s.
    ///
    /// Registered threads with no handle are marked `Stopped` by their own
    /// owner via [`Self::mark`].
    pub fn join_all(&mut self) {
        self.join_all_within(Duration::from_secs(30));
    }

    /// Join adopted threads, but never wait past `grace`.
    ///
    /// A clean shutdown has a deadline: a thread that ignores the stop flag
    /// must not hold the process open forever. Finished threads are joined and
    /// recorded; threads still running at the deadline are detached and marked
    /// `Failed` so shutdown completes and the stall is visible in the summary.
    pub fn join_all_within(&mut self, grace: Duration) {
        let deadline = Instant::now() + grace;
        loop {
            let mut pending = false;
            for entry in &mut self.entries {
                let Some(handle) = entry.handle.as_ref() else {
                    continue;
                };
                if handle.is_finished() {
                    let handle = entry.handle.take().expect("handle was just checked");
                    entry.state = if handle.join().is_ok() {
                        ThreadState::Stopped
                    } else {
                        ThreadState::Failed
                    };
                } else {
                    pending = true;
                }
            }
            if !pending {
                break;
            }
            if Instant::now() >= deadline {
                for entry in &mut self.entries {
                    if entry.handle.is_some() {
                        tracing::warn!(
                            "[manager] thread '{}' did not stop within {:?}; detaching",
                            entry.name,
                            grace
                        );
                        entry.handle = None;
                        entry.state = ThreadState::Failed;
                    }
                }
                break;
            }
            std::thread::sleep(Duration::from_millis(1));
        }
    }

    pub fn statuses(&self) -> Vec<ThreadStatus> {
        self.entries
            .iter()
            .map(|entry| ThreadStatus {
                role: entry.role,
                name: entry.name.clone(),
                state: entry.state,
            })
            .collect()
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    #[inline]
    pub fn all_stopped(&self) -> bool {
        self.entries.iter().all(|entry| entry.state.is_terminal())
    }

    /// One-line summary for shutdown logs, e.g.
    /// `host=running,sim-plane=stopped`.
    pub fn summary(&self) -> String {
        self.entries
            .iter()
            .map(|entry| format!("{}={}", entry.name, entry.state.as_str()))
            .collect::<Vec<_>>()
            .join(",")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;

    #[test]
    fn a_registered_thread_starts_running_and_has_no_handle() {
        let mut r = ThreadRegistry::new();
        r.register(ThreadRole::Host, "cyg-host");
        assert_eq!(r.state_of("cyg-host"), Some(ThreadState::Running));
        assert_eq!(r.len(), 1);
        assert!(!r.all_stopped());
    }

    #[test]
    fn request_stop_flips_the_flag_and_marks_live_threads_stopping() {
        let mut r = ThreadRegistry::new();
        r.register(ThreadRole::Host, "cyg-host");
        r.register(ThreadRole::Control, "cyg-control");
        assert!(!r.stop_requested());
        r.request_stop();
        assert!(r.stop_requested());
        assert!(r.stop_flag().load(Ordering::Relaxed));
        assert_eq!(r.state_of("cyg-host"), Some(ThreadState::Stopping));
        assert_eq!(r.state_of("cyg-control"), Some(ThreadState::Stopping));
    }

    #[test]
    fn a_spawned_thread_observes_the_stop_flag_and_is_joined() {
        let mut r = ThreadRegistry::new();
        let flag = r.stop_flag();
        r.spawn(ThreadRole::Custom, "worker", move || {
            while !flag.load(Ordering::Relaxed) {
                std::thread::yield_now();
            }
        })
        .expect("spawn worker");
        assert_eq!(r.state_of("worker"), Some(ThreadState::Running));
        r.request_stop();
        r.join_all();
        assert_eq!(r.state_of("worker"), Some(ThreadState::Stopped));
        assert!(r.all_stopped());
    }

    #[test]
    fn stop_hooks_run_before_joining() {
        let mut r = ThreadRegistry::new();
        let (tx, rx) = mpsc::channel();
        r.on_stop(move || {
            let _ = tx.send("hook");
        });
        r.request_stop();
        assert_eq!(rx.try_recv().unwrap(), "hook");
    }

    #[test]
    fn a_panicking_thread_is_recorded_as_failed() {
        let mut r = ThreadRegistry::new();
        r.spawn(ThreadRole::Custom, "boom", || panic!("boom"))
            .expect("spawn");
        r.join_all();
        assert_eq!(r.state_of("boom"), Some(ThreadState::Failed));
        assert!(r.all_stopped());
    }

    #[test]
    fn a_thread_that_ignores_the_stop_flag_is_detached_at_the_deadline() {
        let mut r = ThreadRegistry::new();
        let (tx, rx) = mpsc::channel::<()>();
        r.spawn(ThreadRole::Custom, "stuck", move || {
            let _ = rx.recv();
        })
        .expect("spawn");
        r.request_stop();
        let started = Instant::now();
        r.join_all_within(Duration::from_millis(20));
        assert!(
            started.elapsed() < Duration::from_secs(2),
            "shutdown must not block on a stuck thread"
        );
        assert_eq!(r.state_of("stuck"), Some(ThreadState::Failed));
        drop(tx);
    }

    #[test]
    fn summary_names_every_thread() {
        let mut r = ThreadRegistry::new();
        r.register(ThreadRole::Host, "cyg-host");
        r.mark("cyg-host", ThreadState::Stopped);
        assert_eq!(r.summary(), "cyg-host=stopped");
    }
}
