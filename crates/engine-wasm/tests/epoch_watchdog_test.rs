//! The epoch watchdog: a guest call that outruns its budget is cut off, and the
//! instance is still usable afterwards because every call re-arms the fuse.
//!
//! Both call paths a room uses are covered — the dynamic
//! [`engine_wasm::WasmInstance::call`] and the cached `wasm_module!` wrapper
//! that `RoomLogic::on_tick` goes through.

use std::time::Duration;

use engine_wasm::{WasmConfig, WasmPreInstance, WasmRuntime, wasm_module};

/// `spin` counts to its argument on a loop backedge — the place wasmtime puts
/// its epoch checks. The bound is large enough that a working fuse always fires
/// first, and small enough that a broken one finishes instead of hanging.
const GUEST: &str = r#"
(module
    (memory (export "memory") 1)
    (func (export "spin") (param $limit i32) (result i32)
        (local $i i32)
        (block $done
            (loop $again
                (local.set $i (i32.add (local.get $i) (i32.const 1)))
                (br_if $done (i32.ge_u (local.get $i) (local.get $limit)))
                (br $again)))
        (local.get $i))
    (func (export "quick") (result i32) (i32.const 7))
)
"#;

wasm_module! {
    pub struct WatchedGuest {
        fn spin(limit: i32) -> i32;
        fn quick() -> i32;
    }
}

/// Two ticks of budget, pumped every 2 ms: a stuck guest dies inside ~4 ms.
fn watched() -> (WasmRuntime, WasmPreInstance<()>, engine_wasm::EpochTicker) {
    let runtime = WasmRuntime::new_with_config(WasmConfig {
        optimize_for_speed: true,
        enable_epoch_interruption: true,
        epoch_deadline_ticks: Some(2),
        ..WasmConfig::default()
    })
    .expect("watchdog runtime");
    let module = runtime
        .compile(&wat::parse_str(GUEST).expect("guest wat"))
        .expect("compile guest");
    let pre = runtime
        .prepare_standalone::<()>(&module)
        .expect("prepare guest");
    let ticker = runtime.spawn_epoch_ticker(Duration::from_millis(2));
    (runtime, pre, ticker)
}

const STUCK_FOREVER: i32 = 2_000_000_000;

#[test]
fn a_guest_that_outruns_its_budget_is_interrupted() {
    let (_runtime, pre, _ticker) = watched();
    let mut instance = pre.instantiate(()).expect("instantiate guest");
    assert_eq!(instance.epoch_deadline_ticks(), Some(2));

    let outcome = instance.call::<i32, i32>("spin", STUCK_FOREVER);
    let error = outcome.expect_err("a 2e9 iteration loop must not outrun nothing");
    assert!(
        error.is_interrupt(),
        "expected a watchdog kill, got {error}"
    );
}

#[test]
fn an_interrupted_guest_is_armed_again_for_its_next_call() {
    let (_runtime, pre, _ticker) = watched();
    let mut instance = pre.instantiate(()).expect("instantiate guest");
    let _ = instance
        .call::<i32, i32>("spin", STUCK_FOREVER)
        .expect_err("first call is cut off");

    // A deadline is consumed, not refilled: were the fuse not re-armed per
    // call, this short export would trap against the already-elapsed deadline.
    assert_eq!(
        instance
            .call::<(), i32>("quick", ())
            .expect("quick call ok"),
        7
    );
    assert_eq!(
        instance
            .call::<(), i32>("quick", ())
            .expect("quick call ok"),
        7
    );
}

#[test]
fn the_cached_wrapper_the_room_uses_is_watchdoged_too() {
    let (_runtime, pre, _ticker) = watched();
    let mut guest = WatchedGuest::from_pre(&pre, ()).expect("wrap guest");

    let error = guest
        .spin(STUCK_FOREVER)
        .expect_err("the wrapped call is cut off as well");
    assert!(
        error.is_interrupt(),
        "expected a watchdog kill, got {error}"
    );
    assert_eq!(guest.quick().expect("next tick re-arms"), 7);
}

#[test]
fn a_runtime_without_the_watchdog_leaves_the_guest_uninterrupted() {
    let runtime = WasmRuntime::new().expect("default runtime");
    let module = runtime
        .compile(&wat::parse_str(GUEST).expect("guest wat"))
        .expect("compile guest");
    let pre = runtime
        .prepare_standalone::<()>(&module)
        .expect("prepare guest");
    let mut instance = pre.instantiate(()).expect("instantiate guest");

    assert_eq!(instance.epoch_deadline_ticks(), None);
    // 5e7 counts stay quick, and with no instrumentation they all run.
    assert_eq!(
        instance
            .call::<i32, i32>("spin", 50_000_000)
            .expect("unwatched guest completes"),
        50_000_000
    );
}
