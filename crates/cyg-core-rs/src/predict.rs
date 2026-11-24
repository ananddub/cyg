//! Client-side prediction, rollback and reconciliation for cyg-core-rs.
//!
//! # Design provenance
//!
//! The rollback/reconcile loop is a port of the *ideas* in Colyseus's
//! `@colyseus/sdk` `src/predict/` (`rollback.ts` `RollbackController`,
//! `reconciler.ts`, `drift.ts`, `divergence.ts` — MIT-licensed). We credit
//! that design: input buffering with per-seq history, a server-ack driven
//! rewind-and-replay reconcile, and the EMA/peak drift telemetry pair are
//! all modelled on it. The implementation here is deliberately different:
//!
//! - **Game-agnostic over OXS2 entities.** Colyseus binds prediction to
//!   decoded schema instances with a user `step` closure. cyg-core stores
//!   entities as `Vec<Value>` in schema field order, so the game supplies an
//!   [`InputApplier`] that mutates an entity's values given a raw input
//!   (`x += dir * speed`). The core never knows what the fields mean.
//! - **Server truth comes from the delta envelope.** Colyseus polls
//!   `input.lastProcessed` on a JS input handle. Here every payload's
//!   envelope tail carries `[acked_seq u16][server_tick u32]`
//!   (`wire::unwrap_payload` → `ClientCore::last_acked_seq`); the ack and
//!   the authoritative patch arrive in the SAME frame, so reconcile uses the
//!   post-patch entity values as the restore point (the `adoptTruth` role).
//! - **Per-seq predicted-state history ring** instead of Colyseus's
//!   `history` Float64Array + wire-quantizer compare. Divergence is measured
//!   per field: numeric fields as `|predicted − authoritative|` against a
//!   configurable threshold, bools/strings/arrays compared exactly (no
//!   float-equality tricks).
//! - **No smoothing / interpolation / memo / predicted-events.** Those
//!   Colyseus layers (`Predictor.ts` smoothing slots, `ctx.memo`,
//!   `defineEvent`, `spawns`) are presentation-side concerns; bindings own
//!   them. The core exposes the raw reconcile numbers so a debug HUD can be
//!   built on top (`stats()` / `cyg_pred_divergence`).
//!
//! # The canonical client loop
//!
//! ```text
//! spawn:     predictor.capture_pre_state(&state)          // rollback baseline
//! per input: predictor.predict(seq, values)               // buffer
//!            applier.apply(&mut entity, &values)          // predict locally (zero latency)
//!            predictor.record_history(&state, id, seq)    // remember predicted state @seq
//! per payload: core.apply_payload(bytes)                  // authoritative patch lands
//!            let out = predictor.reconcile(&mut state, id, core.last_acked_seq(), &applier);
//!            //   1. divergence = |history[acked] − post-patch truth|
//!            //   2. drop pending inputs with seq <= acked
//!            //   3. adopt server truth into the entity   (rewind)
//!            for each still-pending input (seq > acked):  // replay
//!                applier.apply(&mut entity, &input.values);
//!                predictor.record_history(&state, id, input.seq);
//! ```
//!
//! After reconcile the entity holds the server truth plus every
//! still-unacknowledged input replayed on top — exactly Colyseus's
//! rewind-and-replay — and the game keeps rendering from it.
//!
//! # C ABI
//!
//! The applier callback cannot cross the C ABI as a closure, so
//! reconciliation is GAME-DRIVEN there: `cyg_pred_reconcile` performs the
//! divergence measure + ack drop + adopt, then the game pops the pending
//! inputs (`cyg_pred_pending_count` / `cyg_pred_pending_seq` /
//! `cyg_pred_pop_input`), applies each with its own applier, and calls
//! `cyg_pred_record` after each. Native (`ffi.rs`) and wasm (`wasm_ffi.rs`)
//! export the same symbols.

use std::collections::{BTreeMap, VecDeque};

use crate::state::{ClientState, Entity, Value};

/// Who advances one entity by one input. Implemented by the game (Rust
/// hosts) or by the platform binding's applier shim; the core only calls it.
pub trait InputApplier {
    /// Mutate `entity`'s values in place by applying one input frame.
    /// `input` holds the raw input values in Input-schema field order —
    /// the same encoding `cyg_pred_predict` / `encode_schema_input` carry
    /// (i64s; floats reinterpreted via bit patterns).
    fn apply(&mut self, entity: &mut Entity, input: &[i64]);
}

/// One unacknowledged predicted input.
#[derive(Clone, Debug, PartialEq)]
pub struct PendingInput {
    pub seq: u16,
    /// Raw input values in Input-schema field order (same encoding as
    /// `cyg_encode_schema_input`: i64s, floats carried as bit patterns).
    pub values: Vec<i64>,
}

/// Divergence / drift telemetry — the debug-HUD feed.
///
/// `ema` / `peak` mirror Colyseus `drift.ts`: `ema` is the persistent
/// component (steady nonzero ⇒ the client and server simulations genuinely
/// disagree), `peak` a decaying max (spikes ⇒ network jitter, not divergence).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DivergenceStats {
    /// Max |predicted − authoritative| over numeric fields of the LAST
    /// reconcile (0 = matched). `None`-reconciles (no history at the ack)
    /// leave this untouched.
    pub last_max_drift: f64,
    /// EMA of `last_max_drift` (alpha 0.1, per reconcile).
    pub ema: f64,
    /// Decaying max of `last_max_drift` (factor 0.9, per reconcile).
    pub peak: f64,
    /// Reconciles where the history ring had a predicted state at the acked
    /// seq (i.e. real comparisons happened).
    pub confirmed: u64,
    /// Total inputs buffered by `predict` (the predicted count).
    pub predicted: u64,
    /// Total inputs dropped because the server acked them.
    pub acked_total: u64,
    /// Cumulative exact-compare mismatches (bool/string/array fields or
    /// type changes) seen across reconciles.
    pub mismatch_events: u64,
    /// Whether the last compare had at least one exact-compare mismatch.
    pub last_had_mismatch: bool,
    /// The acked seq the last compare ran at (-1 = never compared).
    pub last_confirmed_seq: i32,
}

/// What one `reconcile` did — the game drives replay from `replay_count`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ReconcileOutcome {
    /// Pending inputs dropped because the server acked them.
    pub acked_dropped: usize,
    /// Pending inputs still outstanding (the game must replay these many).
    pub replay_count: usize,
    /// Divergence measured this reconcile, when a predicted state existed
    /// at the acked seq.
    pub drift: Option<f64>,
    /// Whether the compare included an exact (bool/string) mismatch.
    pub mismatch: bool,
}

/// History ring capacity (per-seq predicted states; ~2s at 60Hz of ack lag).
pub const HISTORY_CAP: usize = 128;

/// Rollback + prediction state for ONE collection (usually the players
/// collection, with the local player's entity as the predicted one).
pub struct Predictor {
    /// Index into `ClientState::collections` this predictor tracks.
    pub collection_slot: usize,
    /// Unacknowledged inputs, ascending seq.
    pending: VecDeque<PendingInput>,
    /// Highest acked seq seen so far (wrap-aware compare anchor).
    last_acked: u16,
    /// Pre-prediction snapshot of the tracked collection — the rollback
    /// baseline (`capture_pre_state` / `restore_rollback`).
    rollback: BTreeMap<u32, Vec<Value>>,
    /// Per-seq predicted entity values ring: slot `seq % HISTORY_CAP`
    /// holds `(seq, values)`; `None` = empty. Written on live predicts
    /// (`record_history`) and on every replay.
    history: [Option<(u16, Vec<Value>)>; HISTORY_CAP],
    pub stats: DivergenceStats,
}

impl Predictor {
    pub fn new(collection_slot: usize) -> Self {
        Self {
            collection_slot,
            pending: VecDeque::new(),
            last_acked: 0,
            rollback: BTreeMap::new(),
            history: std::array::from_fn(|_| None),
            stats: DivergenceStats::default(),
        }
    }

    // --- seq helpers (wrap-aware; u16 seqs wrap at 65536) -------------------

    /// True when `a` is at-or-before `b` on the u16 ring
    /// (`a.wrapping_sub(b)` interpreted as i16 is <= 0).
    fn seq_le(a: u16, b: u16) -> bool {
        (a.wrapping_sub(b) as i16) <= 0
    }

    // --- buffering -----------------------------------------------------------

    /// Buffer one predicted input. Errors on a seq at-or-before the newest
    /// pending one (out-of-order predict) — the replay order would break.
    pub fn predict(&mut self, seq: u16, values: Vec<i64>) -> Result<(), String> {
        if self
            .pending
            .back()
            .is_some_and(|newest| Self::seq_le(seq, newest.seq))
        {
            let newest = self.pending.back().unwrap().seq;
            return Err(format!(
                "predict seq {seq} not after newest pending seq {newest}"
            ));
        }
        self.pending.push_back(PendingInput { seq, values });
        self.stats.predicted += 1;
        Ok(())
    }

    /// Pending inputs (ascending seq), oldest first.
    pub fn pending(&self) -> &VecDeque<PendingInput> {
        &self.pending
    }

    /// Values of the pending input at logical position `pos` (peek — the
    /// queue drains only via acks; a replayed input stays for next time).
    pub fn pending_values(&self, pos: usize) -> Option<&[i64]> {
        self.pending.get(pos).map(|p| p.values.as_slice())
    }

    // --- rollback baseline ---------------------------------------------------

    /// Snapshot EVERY entity of the tracked collection (all `Vec<Value>`
    /// clones) as the pre-prediction rollback baseline.
    pub fn capture_pre_state(&mut self, state: &ClientState) -> usize {
        self.rollback.clear();
        if let Some(coll) = state.collections.get(self.collection_slot) {
            for (id, entity) in &coll.entities {
                self.rollback.insert(*id, entity.values.clone());
            }
        }
        self.rollback.len()
    }

    /// Restore the rollback baseline into the state (entities still present
    /// only — server-removed rows stay removed). Returns rows restored.
    pub fn restore_rollback(&mut self, state: &mut ClientState) -> usize {
        let slot = self.collection_slot;
        let mut restored = 0;
        if let Some(coll) = state.collections.get_mut(slot) {
            for (id, values) in &self.rollback {
                if let Some(entity) = coll.entities.get_mut(id) {
                    entity.values = values.clone();
                    restored += 1;
                }
            }
        }
        restored
    }

    // --- per-seq history -------------------------------------------------------

    /// Record the CURRENT predicted values of `entity_id` as the prediction
    /// for `seq` (call after a live predict-apply and after each replayed
    /// apply). Errors if the entity is absent.
    pub fn record_history(
        &mut self,
        state: &ClientState,
        entity_id: u32,
        seq: u16,
    ) -> Result<(), String> {
        let coll = state
            .collections
            .get(self.collection_slot)
            .ok_or_else(|| format!("no collection slot {}", self.collection_slot))?;
        let entity = coll
            .entities
            .get(&entity_id)
            .ok_or_else(|| format!("no entity {entity_id}"))?;
        let slot = seq as usize % HISTORY_CAP;
        self.history[slot] = Some((seq, entity.values.clone()));
        Ok(())
    }

    fn history_at(&self, seq: u16) -> Option<&Vec<Value>> {
        match &self.history[seq as usize % HISTORY_CAP] {
            Some((s, values)) if *s == seq => Some(values),
            _ => None,
        }
    }

    // --- ack / reconcile ---------------------------------------------------------

    /// Drop pending inputs the server has acked (`seq <= acked_seq`,
    /// wrap-aware). Returns how many were dropped.
    pub fn note_acked(&mut self, acked_seq: u16) -> usize {
        let before = self.pending.len();
        self.pending.retain(|p| !Self::seq_le(p.seq, acked_seq));
        self.last_acked = acked_seq;
        self.stats.acked_total += (before - self.pending.len()) as u64;
        before - self.pending.len()
    }

    /// One reconcile step (the rewind in rewind-and-replay):
    ///
    /// 1. measure divergence between the PREDICTED state at `acked_seq`
    ///    (history ring) and the authoritative post-patch values currently
    ///    in the entity — numeric fields as `|Δ|` (max feeds the stats),
    ///    bools/strings/arrays compared exactly;
    /// 2. drop acked pending inputs;
    /// 3. ADOPT the authoritative values into the entity (the restore
    ///    point) — the game then replays `outcome.replay_count` pending
    ///    inputs through its applier, calling `record_history` after each.
    ///
    /// `threshold` is informational for the stats (drift below it still
    /// reports its exact magnitude); compare is exact `|Δ|` — no
    /// float-equality tricks.
    pub fn reconcile(
        &mut self,
        state: &mut ClientState,
        entity_id: u32,
        acked_seq: u16,
    ) -> ReconcileOutcome {
        let slot = self.collection_slot;
        let truth: Vec<Value> = state
            .collections
            .get(slot)
            .and_then(|c| c.entities.get(&entity_id))
            .map(|e| e.values.clone())
            .unwrap_or_default();

        // 1. divergence at the acked seq
        let mut drift = None;
        let mut mismatch = false;
        if let Some(predicted) = self.history_at(acked_seq) {
            let (max_drift, mism) = compare_values(predicted, &truth);
            mismatch = mism;
            self.stats.confirmed += 1;
            self.stats.last_max_drift = max_drift;
            self.stats.ema += (max_drift - self.stats.ema) * 0.1;
            self.stats.peak = max_drift.max(self.stats.peak * 0.9);
            if mism {
                self.stats.mismatch_events += 1;
            }
            self.stats.last_had_mismatch = mism;
            self.stats.last_confirmed_seq = acked_seq as i32;
            drift = Some(max_drift);
        }

        // 2. drop acked inputs
        let acked_dropped = self.note_acked(acked_seq);

        // 3. adopt server truth (rewind); game replays pending on top
        if let Some(entity) = state
            .collections
            .get_mut(slot)
            .and_then(|c| c.entities.get_mut(&entity_id))
            .filter(|_| !truth.is_empty())
        {
            entity.values = truth;
        }

        ReconcileOutcome {
            acked_dropped,
            replay_count: self.pending.len(),
            drift,
            mismatch,
        }
    }

    /// Current stats snapshot.
    pub fn stats(&self) -> &DivergenceStats {
        &self.stats
    }
}

/// Compare a predicted `Vec<Value>` against the authoritative one.
/// Returns `(max numeric |Δ|, any exact mismatch)`. Numeric kinds measure
/// `|a − b|` via f64; bools/strings/arrays compare exactly; kind changes
/// count as mismatches.
pub fn compare_values(predicted: &[Value], authoritative: &[Value]) -> (f64, bool) {
    let mut max = 0.0f64;
    let mut mismatch = false;
    for (a, b) in predicted.iter().zip(authoritative.iter()) {
        match (a, b) {
            (Value::Bool(x), Value::Bool(y)) => {
                if x != y {
                    mismatch = true;
                }
            }
            (Value::Str(x), Value::Str(y)) => {
                if x != y {
                    mismatch = true;
                }
            }
            (Value::Arr(x), Value::Arr(y)) => {
                let (d, m) = compare_values(x, y);
                max = max.max(d);
                if m {
                    mismatch = true;
                }
            }
            (Value::Null, Value::Null) => {}
            _ => {
                let (na, nb) = (a.get_f64(), b.get_f64());
                match (na, nb) {
                    (Some(x), Some(y)) => max = max.max((x - y).abs()),
                    _ => mismatch = true, // type change or non-comparable
                }
            }
        }
    }
    if predicted.len() != authoritative.len() {
        mismatch = true;
    }
    (max, mismatch)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::StateEvent;
    use engine_schema::{
        RootCollection, RootState, SchemaDef, SchemaField, WireKind, emit_descriptor,
    };

    fn field(name: &str, kind: WireKind) -> SchemaField {
        SchemaField {
            name: name.into(),
            kind,
            element: None,
            ref_target: None,
            flags: 0,
            source_type: String::new(),
            default: None,
            encoding: engine_schema::default_encoding(kind),
            nested_path: None,
            line: 0,
            column: 0,
        }
    }

    /// Player schema: [0] name Str, [1] x F64, [2] y F64, [3] alive Bool.
    fn player_schema() -> SchemaDef {
        SchemaDef {
            nested_arrays: Vec::new(),
            name: "Player".into(),
            version: 1,
            fields: vec![
                field("name", WireKind::String),
                field("x", WireKind::F64),
                field("y", WireKind::F64),
                field("alive", WireKind::Bool),
            ],
            line: 0,
            column: 0,
        }
    }

    fn player_packed() -> engine_webrtc_protocol::packed::PackedSchema {
        engine_webrtc_protocol::packed::PackedSchema::from_def(&player_schema())
    }

    fn player_descriptor() -> Vec<u8> {
        let root = RootState {
            nested: Vec::new(),
            name: "Arena".into(),
            version: 1,
            scalars: vec![],
            collections: vec![RootCollection {
                name: "players".into(),
                schema: "Player".into(),
            }],
        };
        emit_descriptor(&[player_schema()], Some(&root))
    }

    fn full_section(
        packed: &engine_webrtc_protocol::packed::PackedSchema,
        rows: &[(u32, &str, f64, f64, bool)],
    ) -> Vec<u8> {
        use engine_webrtc_protocol::packed::{WireValue, encode_full};
        let rows: Vec<(u32, Vec<WireValue>)> = rows
            .iter()
            .map(|(id, name, x, y, alive)| {
                (
                    *id,
                    vec![
                        WireValue::Str((*name).into()),
                        WireValue::F64(*x),
                        WireValue::F64(*y),
                        WireValue::Bool(*alive),
                    ],
                )
            })
            .collect();
        let mut clamped = 0;
        encode_full(packed, &rows, &mut clamped).expect("packed full")
    }

    /// Packed patch with one UPDATE row: mask 0b1110 → x, y, alive.
    fn update_section(
        packed: &engine_webrtc_protocol::packed::PackedSchema,
        id: u32,
        x: f64,
        y: f64,
        alive: bool,
    ) -> Vec<u8> {
        use engine_webrtc_protocol::packed::{PackedOp, WireValue, encode_patch};
        let mut clamped = 0;
        encode_patch(
            packed,
            &[PackedOp::Update {
                id,
                mask: 0b1110,
                values: vec![
                    (1, WireValue::F64(x)),
                    (2, WireValue::F64(y)),
                    (3, WireValue::Bool(alive)),
                ],
            }],
            &mut clamped,
        )
        .expect("packed patch")
    }

    fn envelope(section: &[u8], acked: u16, tick: u32) -> Vec<u8> {
        let mut p = vec![0xA5u8];
        p.extend_from_slice(b"delta");
        p.extend_from_slice(&1i64.to_le_bytes());
        p.extend_from_slice(section);
        p.extend_from_slice(&acked.to_le_bytes());
        p.extend_from_slice(&tick.to_le_bytes());
        p.extend_from_slice(&0u16.to_le_bytes()); // client stamp
        p
    }

    /// Client-side applier: `x += dir * speed (+ wind)`. Raw i64 input
    /// encoding, f64s carried as bit patterns — same as the wire.
    struct MoveApplier {
        wind: f64,
    }
    impl InputApplier for MoveApplier {
        fn apply(&mut self, entity: &mut Entity, input: &[i64]) {
            let dir = input[0] as f64;
            let speed = f64::from_bits(input[1] as u64);
            let x = entity.values[1].as_f64() + dir * speed + self.wind;
            entity.values[1] = Value::F64(x);
        }
    }

    /// The "server": authoritative `x`, applies inputs per ack, sends patches.
    struct Server {
        packed: engine_webrtc_protocol::packed::PackedSchema,
        x: f64,
        y: f64,
        alive: bool,
        wind: f64,
    }

    impl Server {
        fn apply_input(&mut self, input: &[i64]) {
            let dir = input[0] as f64;
            let speed = f64::from_bits(input[1] as u64);
            self.x += dir * speed + self.wind;
        }
        fn payload(&self, acked: u16, tick: u32) -> Vec<u8> {
            envelope(
                &update_section(&self.packed, 7, self.x, self.y, self.alive),
                acked,
                tick,
            )
        }
        fn full_payload(&self, acked: u16) -> Vec<u8> {
            envelope(
                &full_section(&self.packed, &[(7, "me", self.x, self.y, self.alive)]),
                acked,
                1,
            )
        }
    }

    /// Harness: core + predictor (collection 0) + authoritative server, all
    /// on the entity id 7 of the `players` collection.
    struct Rig {
        core: crate::ClientCore,
        pred: Predictor,
        server: Server,
        /// wind the CLIENT applier applies (0 = the client sim lacks wind)
        client_wind: f64,
    }

    impl Rig {
        fn new(server_wind: f64) -> Self {
            let core = crate::ClientCore::new(&player_descriptor()).unwrap();
            let mut rig = Rig {
                core,
                pred: Predictor::new(0),
                server: Server {
                    packed: player_packed(),
                    x: 0.0,
                    y: 0.0,
                    alive: true,
                    wind: server_wind,
                },
                client_wind: 0.0,
            };
            // baseline full snapshot + rollback capture
            let events = rig.core.apply_payload(&rig.server.full_payload(0)).unwrap();
            assert!(events.iter().any(|e| matches!(e, StateEvent::Ready { .. })));
            rig.pred.capture_pre_state(rig.core.state());
            rig
        }

        /// Client buffers one input, applies it to the core entity via the
        /// applier and records the per-seq predicted history — the exact
        /// shape the FFI game loop drives.
        fn client_predict(&mut self, seq: u16, input: &[i64]) {
            self.pred.predict(seq, input.to_vec()).unwrap();
            self.apply_and_record(seq, input);
        }

        fn apply_and_record(&mut self, seq: u16, input: &[i64]) {
            {
                let wind = self.client_wind;
                let state = self.core.state_mut();
                let entity = state
                    .collections
                    .get_mut(0)
                    .unwrap()
                    .entities
                    .get_mut(&7)
                    .unwrap();
                MoveApplier { wind }.apply(entity, input);
            }
            let state = self.core.state();
            self.pred.record_history(state, 7, seq).unwrap();
        }

        /// One server frame: apply `inputs` authoritatively, send the ack
        /// patch, apply it on the core, reconcile. Returns the outcome.
        fn frame(&mut self, inputs: &[[i64; 2]], acked: u16, tick: u32) -> ReconcileOutcome {
            for input in inputs {
                self.server.apply_input(input);
            }
            let payload = self.server.payload(acked, tick);
            self.core.apply_payload(&payload).unwrap();
            let acked = self.core.last_acked_seq();
            self.pred.reconcile(self.core.state_mut(), 7, acked)
        }

        /// Replay every still-pending input in seq order — the game-driven
        /// half of reconcile after `reconcile()` returns.
        fn replay_pending(&mut self) {
            let n = self.pred.pending().len();
            for pos in 0..n {
                let (seq, values) = {
                    let p = &self.pred.pending()[pos];
                    (p.seq, p.values.clone())
                };
                self.apply_and_record(seq, &values);
            }
        }

        fn client_x(&self) -> f64 {
            self.core.state().collection("players").unwrap().entities[&7].values[1].as_f64()
        }
    }

    fn input(speed: f64) -> [i64; 2] {
        [1, speed.to_bits() as i64]
    }

    #[test]
    fn deterministic_prediction_converges_with_zero_divergence() {
        let mut rig = Rig::new(0.0);
        let step = input(1.0);

        // client predicts 3 steps
        for seq in 1..=3u16 {
            rig.client_predict(seq, &step);
        }
        assert_eq!(rig.pred.pending().len(), 3);
        assert_eq!(rig.client_x(), 3.0);
        assert_eq!(rig.pred.stats().predicted, 3);

        // server applied inputs 1..=2 and acks seq 2 → rewind to truth 2.0,
        // replay of the one pending input brings the client back to 3.0
        let out = rig.frame(&[step, step], 2, 10);
        assert_eq!(out.acked_dropped, 2);
        assert_eq!(out.replay_count, 1);
        assert_eq!(out.drift, Some(0.0));
        assert!(!out.mismatch);
        rig.replay_pending();
        assert_eq!(rig.client_x(), 3.0);
        assert_eq!(rig.pred.stats().last_max_drift, 0.0);

        // ack everything → pending drains, everything matched
        let out = rig.frame(&[step], 3, 20);
        assert_eq!(out.acked_dropped, 1);
        assert_eq!(out.replay_count, 0);
        assert_eq!(out.drift, Some(0.0));
        assert_eq!(rig.pred.pending().len(), 0);
        assert_eq!(rig.pred.stats().acked_total, 3);
        assert_eq!(rig.pred.stats().confirmed, 2);
        assert_eq!(rig.pred.stats().last_max_drift, 0.0);
        assert_eq!(rig.pred.stats().ema, 0.0);
        assert_eq!(rig.pred.stats().peak, 0.0);
        assert_eq!(rig.client_x(), 3.0); // server agrees
    }

    #[test]
    fn server_wind_diverges_then_converges() {
        let mut rig = Rig::new(1.5); // server adds 1.5 per input — the client doesn't (yet)
        let step = input(1.0);

        rig.client_predict(1, &step);
        assert_eq!(rig.client_x(), 1.0);

        // server applied the same input WITH wind → x = 2.5, acks seq 1
        let out = rig.frame(&[step], 1, 10);
        assert_eq!(out.acked_dropped, 1);
        assert_eq!(out.drift, Some(1.5));
        assert_eq!(rig.pred.stats().last_max_drift, 1.5);
        assert_eq!(rig.pred.stats().peak, 1.5);
        assert_eq!(rig.pred.stats().last_confirmed_seq, 1);
        // reconcile ADOPTED server truth: entity back at 2.5, nothing pending
        assert_eq!(out.replay_count, 0);
        assert_eq!(rig.client_x(), 2.5);

        // the game notices the persistent drift and matches the server's wind
        rig.client_wind = 1.5;
        rig.client_predict(2, &step);
        let out = rig.frame(&[step], 2, 20);
        assert_eq!(out.drift, Some(0.0));
        assert_eq!(rig.pred.stats().last_max_drift, 0.0);
        // ema decayed toward 0; peak still remembers the 1.5 spike, one
        // 0.9-decay step later (1.5 * 0.9 = 1.35) — the jitter read
        assert!(rig.pred.stats().ema < 0.2, "ema = {}", rig.pred.stats().ema);
        assert!((rig.pred.stats().peak - 1.35).abs() < 1e-9);
        assert_eq!(out.replay_count, 0);
        assert_eq!(rig.client_x(), 5.0); // 2.5 truth + replayed (1 + wind 1.5)
        assert_eq!(rig.pred.stats().confirmed, 2);
    }

    #[test]
    fn bool_mismatch_is_reported_exactly() {
        let mut rig = Rig::new(0.0);
        let step = input(0.0);

        rig.client_predict(1, &step);
        // server flips `alive` (not input-driven) → exact compare must flag it
        rig.server.alive = false;
        let out = rig.frame(&[], 1, 10);
        assert!(out.mismatch);
        assert_eq!(out.drift, Some(0.0)); // numeric fields still matched
        assert_eq!(rig.pred.stats().mismatch_events, 1);
        assert!(rig.pred.stats().last_had_mismatch);
    }

    #[test]
    fn ack_spanning_multiple_inputs_and_stale_acks() {
        let mut rig = Rig::new(0.0);
        let step = input(1.0);

        for seq in 1..=5u16 {
            rig.client_predict(seq, &step);
        }
        assert_eq!(rig.pred.pending().len(), 5);

        // one ack covering seqs 1..=5 → all five drop in one reconcile
        let out = rig.frame(&[], 5, 30);
        assert_eq!(out.acked_dropped, 5);
        assert_eq!(out.replay_count, 0);
        assert_eq!(rig.pred.stats().acked_total, 5);

        // a stale ack BELOW the watermark drops nothing
        let payload = rig.server.payload(2, 40);
        rig.core.apply_payload(&payload).unwrap();
        let acked = rig.core.last_acked_seq();
        let out = rig.pred.reconcile(rig.core.state_mut(), 7, acked);
        assert_eq!(out.acked_dropped, 0);
        assert_eq!(out.replay_count, 0);

        // seq-space jump: server acks 9 while the client only sent up to 6
        rig.client_predict(6, &step);
        let payload = rig.server.payload(9, 50);
        rig.core.apply_payload(&payload).unwrap();
        let acked = rig.core.last_acked_seq();
        let out = rig.pred.reconcile(rig.core.state_mut(), 7, acked);
        assert_eq!(out.acked_dropped, 1);
        assert_eq!(out.replay_count, 0);
    }

    #[test]
    fn empty_pending_reconciles_cleanly_and_out_of_order_predict_rejected() {
        let mut rig = Rig::new(0.0);

        // no predictions at all: a reconcile over an ack is a no-op with no compare
        let payload = rig.server.payload(3, 10);
        rig.core.apply_payload(&payload).unwrap();
        let acked = rig.core.last_acked_seq();
        let out = rig.pred.reconcile(rig.core.state_mut(), 7, acked);
        assert_eq!(out.acked_dropped, 0);
        assert_eq!(out.replay_count, 0);
        assert_eq!(out.drift, None); // no history at that seq → nothing compared
        assert_eq!(rig.pred.stats().confirmed, 0);

        // out-of-order predict is rejected; the queue stays intact
        rig.pred.predict(5, vec![1]).unwrap();
        assert!(rig.pred.predict(4, vec![1]).is_err());
        assert_eq!(rig.pred.pending().len(), 1);
    }

    #[test]
    fn rollback_restore_recovers_the_baseline() {
        let mut rig = Rig::new(0.0);
        rig.server.x = 10.0;
        // re-baseline at x=10
        rig.core.apply_payload(&rig.server.full_payload(0)).unwrap();
        assert_eq!(rig.pred.capture_pre_state(rig.core.state()), 1);
        assert_eq!(rig.client_x(), 10.0);

        // client predicts past the baseline
        let step = input(5.0);
        rig.client_predict(1, &step);
        assert_eq!(rig.client_x(), 15.0);

        // hard rollback: back to the authoritative baseline
        assert_eq!(rig.pred.restore_rollback(rig.core.state_mut()), 1);
        assert_eq!(rig.client_x(), 10.0);
    }

    #[test]
    fn compare_values_handles_arrays_and_kind_changes() {
        // numeric |Δ|
        let (d, m) = compare_values(
            &[
                Value::F64(1.0),
                Value::F32(2.0),
                Value::Bool(true),
                Value::Str("a".into()),
            ],
            &[
                Value::F64(1.25),
                Value::F32(2.0),
                Value::Bool(true),
                Value::Str("a".into()),
            ],
        );
        assert_eq!(d, 0.25);
        assert!(!m);

        // nested array drift
        let (d, _) = compare_values(
            &[Value::Arr(vec![Value::I32(1), Value::I32(3)])],
            &[Value::Arr(vec![Value::I32(1), Value::I32(5)])],
        );
        assert_eq!(d, 2.0);

        // exact-kind mismatches
        let (_, m) = compare_values(&[Value::Bool(true)], &[Value::Bool(false)]);
        assert!(m);
        let (_, m) = compare_values(&[Value::Str("x".into())], &[Value::Str("y".into())]);
        assert!(m);
        let (_, m) = compare_values(&[Value::F64(1.0)], &[Value::Str("1".into())]);
        assert!(m);
        let (_, m) = compare_values(&[Value::F64(1.0)], &[Value::F64(1.0), Value::Null]);
        assert!(m); // arity difference
    }
}
