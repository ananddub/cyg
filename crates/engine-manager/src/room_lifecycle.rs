use engine_bus::RoomPhase;
use engine_config::LifecycleConfig;
use rustc_hash::FxHashMap;
use std::rc::Rc;
use std::time::{Duration, Instant};

/// Why a room should be closed by the lifecycle manager.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CloseReason {
    Duration,
    Empty,
}

impl CloseReason {
    pub fn as_str(self) -> &'static str {
        match self {
            CloseReason::Duration => "duration",
            CloseReason::Empty => "empty",
        }
    }

    pub fn label(self) -> &'static str {
        self.as_str()
    }
}

pub const DRAIN_REASON: &str = "drain";

#[inline]
pub fn lifecycle_close_code(_reason: CloseReason) -> u16 {
    4101
}

/// Central authoritative lifecycle state for a single room.
///
/// Owns phase, birth/idle timers, pending reconnect-leave deadlines, and
/// reap eligibility. Higher-level room runtime owns membership, simulation,
/// and guest bindings, and delegates lifecycle decisions here.
#[derive(Debug, Clone)]
pub struct RoomLifecycle {
    /// Current lifecycle phase.
    pub phase: RoomPhase,
    /// Creation time of the room instance.
    pub born: Instant,
    /// When the room became empty (no sessions/spectators), if applicable.
    pub empty_since: Option<Instant>,
    /// Per-session pending leave deadlines: session -> (player_id, deadline).
    pub pending_leaves: FxHashMap<Rc<str>, (i64, Instant)>,
    /// Whether this room may be reaped/closed by lifecycle policy.
    pub reapable: bool,
}

impl Default for RoomLifecycle {
    fn default() -> Self {
        Self::new()
    }
}

impl RoomLifecycle {
    /// Creates lifecycle state in the `Created` phase.
    pub fn new() -> Self {
        Self {
            phase: RoomPhase::Created,
            born: Instant::now(),
            empty_since: None,
            pending_leaves: FxHashMap::default(),
            reapable: true,
        }
    }

    #[inline]
    pub fn phase(&self) -> RoomPhase {
        self.phase
    }

    /// Set phase directly (used by explicit transitions that have already been
    /// validated by the caller, mirroring existing room behavior).
    #[inline]
    pub fn set_phase(&mut self, next: RoomPhase) {
        self.phase = next;
    }

    /// Returns `true` when there are no connected sessions, no spectators,
    /// and no pending leave holds.
    #[inline]
    pub fn is_empty(&self, sessions_empty: bool, spectators_empty: bool) -> bool {
        sessions_empty && spectators_empty && self.pending_leaves.is_empty()
    }

    /// Returns `true` when the room is hibernating from a membership point of
    /// view (no sessions/spectators). Pending leave holds keep it non-empty for
    /// occupancy transitions but current runtime treats membership hibernation
    /// as no sessions/spectators.
    #[inline]
    pub fn is_hibernating_membership(&self, sessions_empty: bool, spectators_empty: bool) -> bool {
        sessions_empty && spectators_empty
    }

    /// Update idle timer and phase based on current occupancy.
    ///
    /// Preserves existing behavior: `Created` does not auto-transition here;
    /// other phases become `Active` or `Hibernating` based on whether the room
    /// is fully empty including pending leave holds.
    pub fn note_state(&mut self, now: Instant, sessions_empty: bool, spectators_empty: bool) {
        if sessions_empty && spectators_empty {
            self.empty_since.get_or_insert(now);
        } else {
            self.empty_since = None;
        }

        if self.phase == RoomPhase::Created {
            return;
        }

        let next = if self.is_empty(sessions_empty, spectators_empty) {
            RoomPhase::Hibernating
        } else {
            RoomPhase::Active
        };
        if next != self.phase {
            self.phase = next;
        }
    }

    /// Start the empty-idle timer only when the room is membership-empty.
    pub fn arm_empty_timer(&mut self, now: Instant, sessions_empty: bool, spectators_empty: bool) {
        if self.is_hibernating_membership(sessions_empty, spectators_empty) {
            self.empty_since = Some(now);
        }
    }

    /// Returns `true` if the room has been empty long enough to reap.
    #[inline]
    pub fn reap_check(&self, dur: Duration) -> bool {
        self.reapable && self.empty_since.is_some_and(|t| t.elapsed() >= dur)
    }

    /// Determine whether lifecycle policy requires closing this room.
    pub fn close_reason(
        &self,
        now: Instant,
        lifecycle: &LifecycleConfig,
        sessions_empty: bool,
        spectators_empty: bool,
    ) -> Option<CloseReason> {
        if !self.reapable {
            return None;
        }
        if lifecycle.closes_at(self.born).is_some_and(|at| now >= at) {
            return Some(CloseReason::Duration);
        }
        if self.is_hibernating_membership(sessions_empty, spectators_empty)
            && self.reap_check(Duration::from_secs(lifecycle.reap_idle_secs))
        {
            return Some(CloseReason::Empty);
        }
        None
    }

    pub fn pending_deadline(&self, session: &str) -> Option<Instant> {
        self.pending_leaves.get(session).map(|&(_, at)| at)
    }

    pub fn has_pending_leave(&self, session: &str) -> bool {
        self.pending_leaves.contains_key(session)
    }

    pub fn remove_pending_leave(&mut self, session: &str) -> Option<(i64, Instant)> {
        self.pending_leaves.remove(session)
    }

    pub fn add_pending_leave(&mut self, session: Rc<str>, player_id: i64, deadline: Instant) {
        self.pending_leaves.insert(session, (player_id, deadline));
    }

    pub fn clear_pending_leaves(&mut self) {
        self.pending_leaves.clear();
    }

    /// Drain expired pending leaves, returning `(session, player_id)` for each.
    pub fn take_expired_pending_leaves(&mut self, now: Instant) -> Vec<(Rc<str>, i64)> {
        let expired: Vec<Rc<str>> = self
            .pending_leaves
            .iter()
            .filter_map(|(session, &(_, at))| (now >= at).then(|| Rc::clone(session)))
            .collect();
        expired
            .into_iter()
            .filter_map(|session| {
                self.pending_leaves
                    .remove(&session)
                    .map(|(pid, _)| (session, pid))
            })
            .collect()
    }

    /// Mark the room as created -> active on first successful activation flow
    /// if currently created.
    pub fn activate_if_created(&mut self) {
        if self.phase == RoomPhase::Created {
            self.phase = RoomPhase::Active;
        }
    }

    /// Remove and return all pending leaves as `(session, player_id)`.
    pub fn drain_all_pending_leaves(&mut self) -> Vec<(Rc<str>, i64)> {
        self.pending_leaves
            .drain()
            .map(|(session, (pid, _))| (session, pid))
            .collect()
    }

    #[inline]
    pub fn pending_leaves_count(&self) -> usize {
        self.pending_leaves.len()
    }
}
