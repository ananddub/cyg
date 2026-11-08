use crate::rooms::*;
use std::time::{Duration, Instant};

impl GameRoom {
    #[allow(dead_code)]
    pub(crate) fn pending_deadline(&self, session: &str) -> Option<Instant> {
        self.pending_leaves
            .get(session)
            .map(|(_, deadline)| *deadline)
    }

    pub(crate) fn has_pending_leave(&self, session: &str) -> bool {
        self.pending_leaves.contains_key(session)
    }

    pub(crate) fn rejoin(&mut self, session: &str) -> Option<i64> {
        let (id, _deadline) = self.pending_leaves.remove(session)?;
        if self.sessions.get(session).copied() != Some(id) {
            return None;
        }
        self.note_activity();
        if let Some(ref mut wasm) = self.wasm {
            let _ = wasm
                .raw_instance()
                .call::<(i64,), ()>("on_reconnect", (id,));
        }
        Some(id)
    }

    pub(crate) fn takeover(&mut self, session: &str) -> Option<i64> {
        let id = *self.sessions.get(session)?;
        self.pending_leaves.remove(session);
        self.note_activity();
        if let Some(ref mut wasm) = self.wasm {
            let _ = wasm
                .raw_instance()
                .call::<(i64,), ()>("on_reconnect", (id,));
        }
        Some(id)
    }

    pub(crate) fn expire_pending_leaves(&mut self, now: Instant) -> bool {
        if self.pending_leaves.is_empty() {
            return false;
        }
        let expired: Vec<(std::rc::Rc<str>, i64)> = self
            .pending_leaves
            .iter()
            .filter(|(_, (_, deadline))| now >= *deadline)
            .map(|(session, (id, _))| (session.clone(), *id))
            .collect();
        for (session, id) in &expired {
            self.pending_leaves.remove(session);
            self.sessions.remove(session);
            self.release_player_with(session, *id, close_code::TIMED_OUT);
        }
        if !expired.is_empty() && self.sessions.is_empty() && self.spectators.is_empty() {
            self.empty_since = Some(now);
        }
        !expired.is_empty()
    }

    pub(crate) fn dispose(&mut self) {
        if let Some(ref mut wasm) = self.wasm {
            let _ = wasm.raw_instance().call::<(), ()>("on_dispose", ());
        }
        self.set_state(engine_bus::RoomPhase::Closed);
    }

    /// Pure patch-cadence check (no state mutation): would a patch be due
    /// right now under the slower of the guest's rate and the operator cap?
    pub(crate) fn next_patch_due(&mut self, now: Instant, operator_cap_ms: i32) -> bool {
        let rate = self.guest_patch_rate_ms().max(operator_cap_ms);
        if rate <= 0 {
            return true;
        }
        match self.last_patch_at {
            Some(last) => now.duration_since(last) >= Duration::from_millis(rate as u64),
            None => true,
        }
    }

    /// Tell the guest whether this tick's commit should seal. If the guest
    /// exposes `on_patch_due` (patchDue wired), a not-due commit returns an
    /// empty delta and RETAINS its dirty rows, so the next due patch carries
    /// every change since the last send — no full snapshot needed.
    pub(crate) fn signal_patch_due(&mut self, due: bool) {
        self.probe_guest_fns();
        let Some(func) = &self.on_patch_due_fn else {
            return;
        };
        self.patch_due_wired = true;
        if let Some(ref mut wasm) = self.wasm {
            let _ = func.call(wasm.raw_instance().store_mut(), (due as i32,));
        }
    }

    /// Anchor the next patch window at an actual send.
    pub(crate) fn mark_patch_sent(&mut self, now: Instant) {
        self.last_patch_at = Some(now);
        self.patch_skipped = false;
    }

    pub(crate) fn should_send_patch(&mut self, now: Instant, operator_cap_ms: i32) -> bool {
        // The operator cap can only THROTTLE the room: the slower of the
        // guest's rate and the config's patch_rate wins. 0 on either side
        // means "no constraint from that side".
        let rate = self.guest_patch_rate_ms().max(operator_cap_ms);
        if rate <= 0 {
            self.last_patch_at = Some(now);
            return true;
        }
        let due = match self.last_patch_at {
            Some(last) => now.duration_since(last) >= Duration::from_millis(rate as u64),
            None => true,
        };
        if due {
            self.last_patch_at = Some(now);
        } else {
            self.patch_skipped = true;
        }
        due
    }

    pub(crate) fn take_patch_skipped(&mut self) -> bool {
        std::mem::take(&mut self.patch_skipped)
    }
}
