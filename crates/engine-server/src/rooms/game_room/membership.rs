use crate::rooms::*;
use std::time::{Duration, Instant};

impl GameRoom {
    #[allow(dead_code)]
    pub(crate) fn pending_deadline(&self, session: &str) -> Option<Instant> {
        self.lifecycle.pending_leaves
            .get(session)
            .map(|(_, deadline)| *deadline)
    }

    pub(crate) fn has_pending_leave(&self, session: &str) -> bool {
        self.lifecycle.pending_leaves.contains_key(session)
    }

    pub(crate) fn rejoin(&mut self, session: &str) -> Option<i64> {
        let (id, _deadline) = self.lifecycle.pending_leaves.remove(session)?;
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
        self.lifecycle.pending_leaves.remove(session);
        self.note_activity();
        if let Some(ref mut wasm) = self.wasm {
            let _ = wasm
                .raw_instance()
                .call::<(i64,), ()>("on_reconnect", (id,));
        }
        Some(id)
    }

    pub(crate) fn expire_pending_leaves(&mut self, now: Instant) -> bool {
        if self.lifecycle.pending_leaves.is_empty() {
            return false;
        }
        let expired: Vec<(std::rc::Rc<str>, i64)> = self
            .lifecycle.pending_leaves
            .iter()
            .filter(|(_, (_, deadline))| now >= *deadline)
            .map(|(session, (id, _))| (session.clone(), *id))
            .collect();
        for (session, id) in &expired {
            self.lifecycle.pending_leaves.remove(session);
            self.sessions.remove(session);
            self.release_player_with(session, *id, close_code::TIMED_OUT);
        }
        if !expired.is_empty() && self.sessions.is_empty() && self.spectators.is_empty() {
            self.lifecycle.empty_since = Some(now);
        }
        !expired.is_empty()
    }

    /// Ask the room whether its match produced a result worth scoring.
    ///
    /// Read before `dispose`, because disposing calls `on_dispose` and a guest
    /// is entitled to clear its own state there. A room that never called
    /// `endMatch` answers 0, which is a normal answer — an abandoned round has
    /// nothing to score and not every game has a win condition.
    ///
    /// A result the host cannot read is logged and dropped rather than thrown:
    /// the outside world has already moved on, and a guest bug must not become
    /// a crash in the shutdown path.
    pub(crate) fn take_match_outcome(&mut self) -> Option<super::match_outcome::MatchOutcome> {
        let wasm = self.wasm.as_mut()?;
        // Probed rather than declared on the module struct: a room built
        // against an older SDK has no such export, and requiring it would make
        // every existing room fail to boot.
        let instance = wasm.raw_instance();
        let func = match instance.get_typed_func::<(), i64>("on_match_end") {
            Ok(func) => func,
            Err(_) => return None,
        };
        let packed = match func.call(instance.store_mut(), ()) {
            Ok(value) => value,
            Err(error) => {
                tracing::warn!("[server] on_match_end trapped: {error}");
                return None;
            }
        };
        if packed == 0 {
            return None;
        }

        let ptr = (packed >> 32) as u32;
        let len = packed as u32;
        // A guest answering with a pointer outside its own memory is a guest
        // bug; bounding it here keeps the read from being the thing that first
        // notices.
        if len as usize > super::match_outcome::MAX_RESULT_BYTES {
            tracing::warn!("[server] on_match_end returned {len} bytes — refused");
            return None;
        }
        let bytes = match wasm.read_memory(ptr, len) {
            Ok(bytes) => bytes,
            Err(error) => {
                tracing::warn!("[server] on_match_end memory read failed: {error}");
                return None;
            }
        };

        match super::match_outcome::decode(&bytes) {
            Ok(outcome) => Some(outcome),
            Err(problem) => {
                tracing::warn!("[server] on_match_end result refused: {problem}");
                None
            }
        }
    }

    /// Take the round's result and keep it for the match report.
    ///
    /// Stored on the room rather than sent here, because the report carries
    /// more than the result — end reason, tick count, peak players — and the
    /// bus wants them as one message. A room that reported nothing leaves
    /// `result` empty and the report says so.
    pub(crate) fn collect_match_result(&mut self) {
        let Some(outcome) = self.take_match_outcome() else {
            return;
        };
        tracing::info!(
            "[server] match ended: {} player(s){}",
            outcome.participants.len(),
            outcome
                .leaderboard
                .as_deref()
                .map(|board| format!(", board '{board}'"))
                .unwrap_or_default()
        );
        self.result = Some(bytes::Bytes::from(
            crate::rooms::game_room::match_outcome::encode(&outcome),
        ));
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
