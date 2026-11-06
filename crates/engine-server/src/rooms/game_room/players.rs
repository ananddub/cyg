use crate::rooms::*;
use std::time::Instant;

impl GameRoom {

    pub(crate) fn release_player(&mut self, session: &str, id: i64) {
        self.release_player_with(session, id, close_code::LEFT);
    }

    /// The room sees `onLeave(id, code)` — a [`close_code`] value: 1000 when
    /// the client left on purpose, 1001 on server shutdown, 1005 when its
    /// reconnection window expired, 1006 on a lost connection. Keep the
    /// player's state for `allowReconnection` on anything but LEFT.
    pub(crate) fn release_player_with(&mut self, session: &str, id: i64, code: i32) {
        if let Some(ref mut ecs) = self.ecs {
            ecs.remove_player(session);
        }
        if let Some(ref mut wasm) = self.wasm {
            let _ = wasm.on_leave(id, code);
        }
        self.voice_room.leave_participant(id as u32);
        self.last_input_seq.remove(session);
        self.player_options.remove(&id);
        self.auth_data.remove(&id);
        self.input_journal.retain(|j| j.session != session);
    }

    #[allow(dead_code)]
    pub(crate) fn spawn_point(&self, width: f64, height: f64) -> (f64, f64) {
        let n = self.sessions.len();
        (
            60.0 + ((n as f64) * 97.0) % (width - 120.0),
            60.0 + ((n as f64) * 61.0) % (height - 120.0),
        )
    }

    /// Seat a player. `(x, y)` is the host-chosen spawn (voice/ecs/seat keep
    /// using it); the guest receives the **client's own join options** instead
    /// — raw JSON bytes when the schema has no `JoinOptions` def, the
    /// schema-encoded frame when it does (`decodeJoinOptions`) — plus the
    /// auth data `onAuth` returned, and decides the spawn itself.
    pub(crate) fn join(&mut self, session: &str, x: f64, y: f64, options: &[u8]) -> i64 {
        let id = self.next_player_id;
        self.next_player_id += 1;
        self.empty_since = None;
        if let Some(ref mut ecs) = self.ecs {
            ecs.add_player(session, id, x, y);
        }
        let auth = self.auth_data.get(&id).cloned().unwrap_or_default();
        if let Some(ref mut wasm) = self.wasm {
            Self::call_on_join(wasm, id, options, &auth);
        }
        self.voice_room
            .join_participant(id as u32, Some(engine_voice::hash_team("1")), [x, 0.0, y]);
        self.sessions.insert(session.to_string(), id);
        self.player_options.insert(id, options.to_vec());
        id
    }

    /// Write `options` + `auth` into guest memory and fire
    /// `on_join(id, opt_ptr, opt_len, auth_ptr, auth_len)`; modules still
    /// exporting the ABI v1 `on_join(id, ptr, len)` get the options only.
    #[doc(hidden)]
    pub fn call_on_join(
        wasm: &mut RoomLogic<RoomHostData>,
        id: i64,
        bytes: &[u8],
        auth: &[u8],
    ) {
        let opt = Self::write_guest_bytes(wasm, bytes, "join options");
        let auth_ref = Self::write_guest_bytes(wasm, auth, "auth data");
        let instance = wasm.raw_instance();
        if let Ok(func) = instance
            .get_typed_func::<(i64, i32, i32, i32, i32), ()>("on_join")
        {
            let _ = func.call(instance.store_mut(), (id, opt.0, opt.1, auth_ref.0, auth_ref.1));
            return;
        }
        if let Ok(func) = instance.get_typed_func::<(i64, i32, i32), ()>("on_join") {
            let _ = func.call(instance.store_mut(), (id, opt.0, opt.1));
            return;
        }
        tracing::error!("[server] ⚠️ guest exports no usable on_join for player {id}");
    }

    /// Copy bytes into guest memory, returning `(ptr, len)` — `(0, 0)` when
    /// empty or on failure (the guest treats ptr 0 as absent).
    pub(crate) fn write_guest_bytes(
        wasm: &mut RoomLogic<RoomHostData>,
        bytes: &[u8],
        label: &str,
    ) -> (i32, i32) {
        if bytes.is_empty() {
            return (0, 0);
        }
        match wasm.on_alloc(bytes.len() as i32) {
            Ok(ptr) => {
                if let Err(e) = wasm.write_memory(ptr as u32, bytes) {
                    tracing::error!("[server] ⚠️ {label} write failed: {e}");
                    return (0, 0);
                }
                (ptr, bytes.len() as i32)
            }
            Err(e) => {
                tracing::error!("[server] ⚠️ on_alloc for {label} failed: {e}");
                (0, 0)
            }
        }
    }

    /// Fire `on_create(max, ptr, len)` with the merged create options;
    /// modules still exporting the ABI v1 `on_create(max)` get that.
    #[doc(hidden)]
    pub fn call_on_create(wasm: &mut RoomLogic<RoomHostData>, max: i32, options: &[u8]) {
        let (ptr, len) = Self::write_guest_bytes(wasm, options, "create options");
        let instance = wasm.raw_instance();
        if let Ok(func) = instance.get_typed_func::<(i32, i32, i32), ()>("on_create") {
            let _ = func.call(instance.store_mut(), (max, ptr, len));
            return;
        }
        if let Ok(func) = instance.get_typed_func::<(i32,), ()>("on_create") {
            let _ = func.call(instance.store_mut(), (max,));
            return;
        }
        tracing::error!("[server] ⚠️ guest exports no usable on_create");
    }

    #[allow(dead_code)]
    pub(crate) fn join_spectator(&mut self, session: &str) {
        self.empty_since = None;
        self.spectators.insert(session.to_string());
    }

    pub(crate) fn make_spectator(&mut self, session: &str) {
        if let Some(id) = self.sessions.remove(session) {
            if let Some(ref mut wasm) = self.wasm {
                let _ = wasm.on_leave(id, close_code::LEFT);
            }
            self.player_options.remove(&id);
            self.auth_data.remove(&id);
        }
        self.schema_sent.remove(session);
        self.spectators.insert(session.to_string());
    }

    pub(crate) fn add_raw_player(&mut self, _x: f64, _y: f64) -> i64 {
        let id = self.next_player_id;
        self.next_player_id += 1;
        self.empty_since = None;
        if let Some(ref mut wasm) = self.wasm {
            Self::call_on_join(wasm, id, b"", b"");
        }
        id
    }

    pub(crate) fn remove_raw_player(&mut self, id: i64) {
        if let Some(ref mut wasm) = self.wasm {
            let _ = wasm.on_leave(id, close_code::LEFT);
        }
        self.player_options.remove(&id);
        self.auth_data.remove(&id);
    }

    /// Remove a player the room kicked via `client.leave()` — the seat dies
    /// immediately (no reconnection hold) and the room itself sees
    /// `onLeave(id, CloseCode.KICKED)`.
    pub(crate) fn kick_player(&mut self, session: &str, id: i64) {
        self.detach_session(session);
        self.pending_leaves.remove(session);
        self.sessions.remove(session);
        self.release_player_with(session, id, close_code::KICKED);
    }

    /// Queued `Client.leave()` calls from the last guest invocation.
    pub(crate) fn drain_kicks(&mut self) -> Vec<i64> {
        match self.wasm.as_mut() {
            Some(wasm) => std::mem::take(&mut wasm.data_mut().kick_requests),
            None => Vec::new(),
        }
    }

    pub(crate) fn detach_session(&mut self, session: &str) -> Option<u32> {
        self.spectators.remove(session);
        self.schema_sent.remove(session);
        self.acked_event_seq.remove(session);
        self.voice_mesh.leave_session(session)
    }

    pub(crate) fn arm_empty_timer(&mut self) {
        if self.sessions.is_empty() && self.spectators.is_empty() {
            self.empty_since = Some(Instant::now());
        }
    }

    /// `on_drop(id, code)` — optional export: the client disconnected without
    /// consent and the reconnection hold just started. Rooms without the
    /// export (older modules) are skipped silently.
    pub(crate) fn guest_on_drop(&mut self, id: i64, code: i32) {
        if let Some(ref mut wasm) = self.wasm {
            let instance = wasm.raw_instance();
            if let Ok(func) = instance.get_typed_func::<(i64, i32), ()>("on_drop") {
                let _ = func.call(instance.store_mut(), (id, code));
            }
        }
    }

    /// `on_before_shutdown()` — optional export: the process is draining; the
    /// room gets one outbox flush after this to say goodbye.
    pub(crate) fn guest_on_before_shutdown(&mut self) {
        if let Some(ref mut wasm) = self.wasm {
            let instance = wasm.raw_instance();
            if let Ok(func) = instance.get_typed_func::<(), ()>("on_before_shutdown") {
                let _ = func.call(instance.store_mut(), ());
            }
        }
    }

    /// `on_uncaught_exception(msg_ptr, msg_len, method_ptr, method_len)` —
    /// optional export, best effort: the guest call trapped (watchdog fuse or
    /// guest error) and the instance may be tainted, so a failed notification
    /// is swallowed; the host log already carries the details.
    pub(crate) fn guest_on_uncaught_exception(&mut self, message: &str, method: &str) {
        if let Some(ref mut wasm) = self.wasm {
            let msg = Self::write_guest_bytes(wasm, message.as_bytes(), "exception message");
            let method_ref = Self::write_guest_bytes(wasm, method.as_bytes(), "exception method");
            let instance = wasm.raw_instance();
            if let Ok(func) = instance
                .get_typed_func::<(i32, i32, i32, i32), ()>("on_uncaught_exception")
            {
                let _ = func.call(
                    instance.store_mut(),
                    (msg.0, msg.1, method_ref.0, method_ref.1),
                );
            }
        }
    }

}
