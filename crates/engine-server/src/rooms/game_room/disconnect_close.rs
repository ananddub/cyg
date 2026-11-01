use crate::rooms::*;
use std::time::{Duration, Instant};

impl GameRoom {

    pub(crate) fn handle_disconnect(&mut self, session: &str) -> Option<u32> {
        let gone = self.detach_session(session);
        self.pending_leaves.remove(session);
        if let Some(id) = self.sessions.remove(session) {
            self.release_player(session, id);
        }
        self.arm_empty_timer();
        gone
    }

    pub(crate) fn handle_unexpected_disconnect(
        &mut self,
        session: &str,
        max_hold: Duration,
    ) -> Option<u32> {
        let gone = self.detach_session(session);
        if let Some(id) = self.sessions.get(session).copied() {
            let hold = self
                .guest_allow_reconnection(id)
                .map(|ms| Duration::from_millis(ms as u64).min(max_hold))
                .filter(|hold| hold > &Duration::ZERO);
            if let Some(hold) = hold {
                let deadline = Instant::now() + hold;
                self.pending_leaves
                    .insert(session.to_string(), (id, deadline));
            } else {
                self.sessions.remove(session);
                self.release_player(session, id);
            }
        }
        self.arm_empty_timer();
        gone
    }

    pub(crate) fn note_activity(&mut self) {
        self.empty_since = None;
    }

    pub(crate) fn reap_check(&mut self, timeout: Duration) -> bool {
        if !(self.sessions.is_empty() && self.spectators.is_empty()) {
            return false;
        }
        match self.empty_since {
            Some(t) if t.elapsed() > timeout => true,
            Some(_) => false,
            None => {
                self.empty_since = Some(Instant::now());
                false
            }
        }
    }

    pub(crate) fn close_reason(
        &mut self,
        lifecycle: &engine_config::LifecycleConfig,
        reapable: bool,
        now: Instant,
    ) -> Option<CloseReason> {
        if !reapable {
            return None;
        }
        if lifecycle
            .closes_at(self.born)
            .is_some_and(|deadline| now >= deadline)
        {
            return Some(CloseReason::Duration);
        }
        if self.is_hibernating() && self.reap_check(Duration::from_secs(lifecycle.reap_idle_secs)) {
            return Some(CloseReason::Empty);
        }
        None
    }

}
