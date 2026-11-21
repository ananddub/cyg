//! Client-side prediction parity gate (Ignatchenko discipline).
//!
//! The SAME guest wasm that runs authoritatively also runs client-side (inside
//! Godot/Unity/Unreal via an embedded runtime) as the prediction brain. This
//! test drives the checked-in test guest (tests/guest_as/room.wasm) through:
//!
//!   (a) clean-phase parity — predict + reconcile with no perturbation must
//!       report drift ~0 (bit-equal here, both sides run the shared step from
//!       the same f32 wire lattice);
//!   (b) telemetry — injected divergence raises drift.ema / shows in the
//!       correction;
//!   (c) convergence — after reconcile the mirror equals server truth
//!       bit-for-bit, and replaying pending inputs is bit-identical to the
//!       predict path (replay ≡ predict).
//!
//! Rebuild the guest (asc 0.28.x, `--use CYG_CLIENT_MODE=1` produces the
//! client-mode build):
//!
//!   cd tests/guest_as && asc room.ts -o room.wasm --runtime stub -Ospeed --noAssert
//!
//! (the guest SDK is imported from sdk/server/assemblyscript/cyg-server.ts;
//! add `--use CYG_CLIENT_MODE=1` for the client-mode compile).

use engine_wasm::{WasmInstance, WasmRuntime};
use wasmtime::{ExternType, Func, Linker, Store, ValType};

struct Host;

/// A stub host: every `env.*` import returns zeros. The predict path must
/// never call any of them (the shared step is pure), so a stray host call
/// silently degrades instead of breaking the parity signal.
fn instantiate_with_stubs(runtime: &WasmRuntime, module: &wasmtime::Module) -> WasmInstance<Host> {
    let engine = runtime.raw_engine().clone();
    let mut store = Store::new(&engine, Host);
    let mut linker: Linker<Host> = Linker::new(&engine);
    for imp in module.imports() {
        if let ExternType::Func(ft) = imp.ty() {
            // declared result types (the pre-populated result slots carry
            // null refs, not the declared types)
            let results_ty: Vec<ValType> = ft.results().collect();
            let func = Func::new(&mut store, ft.clone(), move |_, _, results| {
                // zero every result (log/alloc stubs never feed state back)
                for (r, ty) in results.iter_mut().zip(&results_ty) {
                    match ty {
                        ValType::I32 => *r = 0i32.into(),
                        ValType::I64 => *r = 0i64.into(),
                        ValType::F32 => *r = 0f32.into(),
                        ValType::F64 => *r = 0f64.into(),
                        _ => {} // reference types stay null
                    }
                }
                Ok(())
            });
            linker
                .define(&mut store, imp.module(), imp.name(), func)
                .expect("define stub import");
        }
    }
    let instance = linker
        .instantiate(&mut store, module)
        .expect("instantiate guest with stub host");
    WasmInstance::new(store, instance)
}

/// Thin typed wrapper over the guest's predict/reconcile ABI.
struct Guest {
    inst: WasmInstance<Host>,
}

impl Guest {
    fn new(runtime: &WasmRuntime, module: &wasmtime::Module) -> Self {
        Guest {
            inst: instantiate_with_stubs(runtime, module),
        }
    }

    fn call0(&mut self, name: &str) -> f64 {
        self.inst.call::<(), f64>(name, ()).expect(name)
    }

    fn create(&mut self, max: i32) {
        self.inst
            .call::<(i32, i32, i32), ()>("on_create", (max, 0i32, 0i32))
            .expect("on_create");
    }

    fn join(&mut self, id: i64) {
        self.inst
            .call::<(i64, i32, i32, i32, i32), ()>("on_join", (id, 0i32, 0i32, 0i32, 0i32))
            .expect("on_join");
    }

    /// One authoritative frame: write the input frame into guest scratch,
    /// deliver it via on_input, then tick. Returns the reconcile correction.
    fn server_frame(&mut self, alloc: i32, cmd: u32, angle: f32) -> f64 {
        self.write_frame(alloc, cmd, angle);
        self.inst
            .call::<(i64, i32, i32), ()>("on_input", (1i64, alloc, 8i32))
            .expect("on_input");
        let _delta: i64 = self
            .inst
            .call::<f64, i64>("on_tick", 0.05f64)
            .expect("on_tick");
        self.last_correction()
    }

    fn write_frame(&mut self, ptr: i32, cmd: u32, angle: f32) {
        let mut bytes = [0u8; 8];
        bytes[0..4].copy_from_slice(&cmd.to_le_bytes());
        bytes[4..8].copy_from_slice(&angle.to_le_bytes());
        self.inst
            .write_memory(ptr as u32, &bytes)
            .expect("write frame");
    }

    fn scratch(&mut self, len: i32) -> i32 {
        self.inst
            .call::<i32, i32>("on_alloc", len)
            .expect("on_alloc")
    }

    fn last_correction(&mut self) -> f64 {
        self.call0("test_last_correction")
    }

    fn drift_ema(&mut self) -> f64 {
        self.call0("cyg_drift_ema")
    }

    fn drift_peak(&mut self) -> f64 {
        self.call0("cyg_drift_peak")
    }

    fn auth_field(&mut self, i: i32) -> f64 {
        self.inst
            .call::<i32, f64>("test_auth_field", i)
            .expect("test_auth_field")
    }

    /// Predicted mirror field `i`, read back through linear memory.
    fn mirror_field(&mut self, i: i32) -> f64 {
        let ptr: i32 = self
            .inst
            .call::<(), i32>("cyg_predicted_state_ptr", ())
            .expect("state ptr");
        let bytes = self
            .inst
            .read_memory((ptr + i * 8) as u32, 8)
            .expect("read mirror");
        f64::from_le_bytes(bytes.try_into().expect("8 bytes"))
    }

    fn predicted_state_len(&mut self) -> i32 {
        self.inst
            .call::<(), i32>("cyg_predicted_state_len", ())
            .expect("state len")
    }
}

/// Small deterministic LCG — "random-ish" but reproducible scripted inputs.
struct Lcg(u64);

impl Lcg {
    fn next_f64(&mut self) -> f64 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        ((self.0 >> 33) as f64) / (u32::MAX as f64 + 1.0)
    }
}

const CLEAN_FRAMES: usize = 64;

#[test]
fn prediction_parity_and_reconciliation() {
    let runtime = WasmRuntime::new().expect("runtime init");
    let wasm_bytes = include_bytes!("../../../tests/guest_as/room.wasm");
    let module = runtime.compile(wasm_bytes).expect("compile guest");
    let mut g = Guest::new(&runtime, &module);

    g.create(16);
    g.join(1);

    // One grow-only guest scratch window (Room.onAlloc reuses it) — the
    // input frame, the staged server snapshot and the pending buffer are
    // offsets inside the same window.
    let scratch = g.scratch(4096);
    assert!(scratch > 0, "guest scratch alloc");
    let frame_slot = scratch;
    let snapshot = scratch + 512;

    // ------------------------------------------------------------------
    // (a) Clean phase: scripted inputs through the authoritative path.
    //     The room predicts each frame on arrival and reconciles on tick;
    //     with the shared step on both sides every correction is 0.
    // ------------------------------------------------------------------
    let mut rng = Lcg(0xC0FFEE);
    for i in 0..CLEAN_FRAMES {
        let cmd = if rng.next_f64() > 0.3 { 1 } else { 0 };
        let angle = (rng.next_f64() * std::f64::consts::TAU - std::f64::consts::PI) as f32;
        let corr = g.server_frame(frame_slot, cmd, angle);
        assert!(
            corr.abs() < 1e-6,
            "tick {}: phantom rollback, correction {} (f32 lattice breach)",
            i,
            corr
        );
    }

    // Drift telemetry decays from the join-time snapshot adoption spike and
    // stays near zero in the clean phase.
    assert_eq!(
        g.predicted_state_len(),
        24,
        "mirror must expose 3 f64 fields"
    );

    let ema_clean = g.drift_ema();
    let peak_clean = g.drift_peak();
    assert!(ema_clean < 0.05, "clean-phase ema too high: {}", ema_clean);
    assert!(
        peak_clean < 10.0,
        "clean-phase peak too high: {}",
        peak_clean
    );

    // ------------------------------------------------------------------
    // (b) Divergence injection: shove the server truth off the mirror.
    //     The next reconcile must report the correction and lift the EMA.
    // ------------------------------------------------------------------
    g.inst
        .call::<(i32, f64), ()>("test_perturb_self", (0i32, 0.37f64))
        .expect("perturb");
    let corr_injected = g.server_frame(frame_slot, 1, 0.5);
    assert!(
        (corr_injected - 0.37).abs() < 0.01,
        "injected divergence not reported: {}",
        corr_injected
    );
    let ema_after = g.drift_ema();
    assert!(
        ema_after > ema_clean * 2.0,
        "ema did not respond to divergence: {} -> {}",
        ema_clean,
        ema_after
    );

    // ------------------------------------------------------------------
    // (c) Convergence: with injection stopped, corrections return to 0 and
    //     the mirror equals the server truth bit-for-bit.
    // ------------------------------------------------------------------
    for i in 0..5 {
        let corr = g.server_frame(frame_slot, 1, 0.75);
        assert!(corr.abs() < 1e-6, "tick {}: not converged ({})", i, corr);
    }
    let auth_x = g.auth_field(0);
    let mirror_x = g.mirror_field(0);
    assert_eq!(
        auth_x.to_bits(),
        mirror_x.to_bits(),
        "mirror does not equal server truth bit-for-bit ({} vs {})",
        auth_x,
        mirror_x
    );

    // ------------------------------------------------------------------
    // (d) Replay ≡ predict: adopting a server snapshot then REPLAYING a
    //     pending frame through cyg_reconcile lands on exactly the same
    //     state as predicting that frame the normal way.
    // ------------------------------------------------------------------
    // stage the current truth as the "wire snapshot"
    for i in 0..3 {
        let v = g.auth_field(i);
        let bytes = v.to_le_bytes();
        let base = (snapshot + i * 8) as u32;
        g.inst.write_memory(base, &bytes).expect("stage snapshot");
    }

    // predict path: normal frame
    let frame_angle = 1.234f32;
    let corr = g.server_frame(frame_slot, 1, frame_angle);
    assert!(corr.abs() < 1e-6, "predict-path tick not parity-clean");
    let predicted_x = g.mirror_field(0);

    // replay path: reset the mirror to the snapshot (adopt without pending),
    // then hand the SAME frame back as an explicit pending buffer
    g.inst
        .call::<(), ()>("cyg_predict_flush_pending", ())
        .expect("flush");
    let corr_reset: f64 = g
        .inst
        .call::<(i32, i32, i32, i32), f64>("cyg_reconcile", (snapshot, 24i32, 0i32, 0i32))
        .expect("reset reconcile");
    // the reset legitimately reports one predicted frame of catch-up
    assert!(
        corr_reset > 0.0 && corr_reset < 2.0,
        "reset reconcile correction implausible: {}",
        corr_reset
    );

    // pending wire format: [u16 len][f64 dt_ms][8-byte frame]
    let pend_ptr = scratch + 1024;
    let mut pend = [0u8; 18];
    pend[0..2].copy_from_slice(&8u16.to_le_bytes());
    pend[2..10].copy_from_slice(&50.0f64.to_le_bytes());
    let mut frame = [0u8; 8];
    frame[0..4].copy_from_slice(&1u32.to_le_bytes());
    frame[4..8].copy_from_slice(&frame_angle.to_le_bytes());
    pend[10..18].copy_from_slice(&frame);
    g.inst
        .write_memory(pend_ptr as u32, &pend)
        .expect("stage pending");

    let corr_replay: f64 = g
        .inst
        .call::<(i32, i32, i32, i32), f64>("cyg_reconcile", (snapshot, 24i32, pend_ptr, 18i32))
        .expect("replay reconcile");
    assert!(
        corr_replay.abs() < 1e-6,
        "replay reconcile drifted: {}",
        corr_replay
    );

    let replayed_x = g.mirror_field(0);
    assert_eq!(
        predicted_x.to_bits(),
        replayed_x.to_bits(),
        "replay != predict ({} vs {}) — replay path is not the shared step",
        predicted_x,
        replayed_x
    );

    // ------------------------------------------------------------------
    // (e) Wire precision: a server snapshot 0.5 ahead is adopted through
    //     the f32 lattice; the correction reports exactly the wire delta.
    // ------------------------------------------------------------------
    let shifted = auth_x_at(&mut g, snapshot, 0.5);
    let corr_wire: f64 = g
        .inst
        .call::<(i32, i32, i32, i32), f64>("cyg_reconcile", (snapshot, 24i32, 0i32, 0i32))
        .expect("wire reconcile");
    assert!(
        (corr_wire - 0.5).abs() < 1e-6,
        "wire-precision correction wrong: {}",
        corr_wire
    );
    let adopted = g.mirror_field(0);
    assert_eq!(
        shifted.to_bits(),
        adopted.to_bits(),
        "wire snapshot not adopted bit-for-bit"
    );
}

/// Writes `field0 + 0.5` into the staged snapshot slot 0 and returns the
/// f32-rounded value the guest should adopt.
fn auth_x_at(g: &mut Guest, snapshot: i32, delta: f64) -> f64 {
    let v = g.auth_field(0) + delta;
    let bytes = v.to_le_bytes();
    g.inst
        .write_memory(snapshot as u32, &bytes)
        .expect("stage shifted snapshot");
    (v as f32) as f64
}
