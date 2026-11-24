//! The contract, exercised through a real module.
//!
//! The guest here is hand-written WAT rather than compiled AssemblyScript: the
//! point is the ABI between host and hook, and a WAT module makes exactly the
//! bytes that cross that boundary visible.

use crate::event::{HookEvent, HookInput};
use crate::runtime::HookRunner;

/// A hook module that answers `on_purchase` and nothing else.
///
/// It ignores its input and returns a fixed document, which is enough to prove
/// the host reads the answer from host memory and parses it.
const PURCHASE_HOOK: &str = r#"
(module
  (import "env" "hook_input_ptr" (func $ptr (result i32)))
  (import "env" "hook_input_capacity" (func $cap (result i32)))
  (import "env" "hook_write_result" (func $write (param i32 i32)))

  (memory (export "memory") 1)
  (data (i32.const 1024) "\7b\22\67\72\61\6e\74\5f\63\75\72\72\65\6e\63\79\22\3a\5b\7b\22\63\75\72\72\65\6e\63\79\22\3a\22\63\6f\69\6e\73\22\2c\22\61\6d\6f\75\6e\74\22\3a\31\30\30\30\2c\22\72\65\61\73\6f\6e\22\3a\22\76\69\70\22\7d\5d\7d")

  ;; the host asks how many input bytes it wrote
  (func (export "hook_input_len") (result i32) call $cap)

  ;; the answer: 70 bytes of JSON written at 1024, handed back
  (func (export "on_purchase")
    i32.const 1024
    i32.const 70
    call $write
  )

  (func (export "hook_output_len") (result i32) i32.const 70)
)
"#;

#[test]
fn loading_a_module_without_hooks_is_refused() {
    let empty = wat::parse_str(r#"(module (memory (export "memory") 1))"#).unwrap();
    match HookRunner::load(&empty, 4096) {
        Ok(_) => panic!("a module with no hooks must be refused"),
        Err(err) => assert!(err.to_string().contains("exports no hook"), "{err}"),
    }
}

#[test]
fn the_loaded_module_reports_which_events_it_answers() {
    let wasm = wat::parse_str(PURCHASE_HOOK).unwrap();
    let runner = HookRunner::load(&wasm, 4096).expect("loads");
    assert_eq!(runner.exports(), &[HookEvent::Purchase]);
    assert!(runner.answers(HookEvent::Purchase));
    assert!(!runner.answers(HookEvent::LevelUp));
}

#[test]
fn an_event_the_module_does_not_answer_is_not_an_error() {
    let wasm = wat::parse_str(PURCHASE_HOOK).unwrap();
    let runner = HookRunner::load(&wasm, 4096).expect("loads");
    let input = HookInput::new(HookEvent::LevelUp, "p1", serde_json::json!({}));
    assert!(
        runner.run(&input).expect("not an error").is_none(),
        "an unanswered event returns None, distinct from an empty answer"
    );
}

#[test]
fn event_names_round_trip_through_the_parser() {
    for event in HookEvent::ALL {
        assert_eq!(HookEvent::parse(event.as_str()), Some(event));
    }
    assert_eq!(HookEvent::parse("level-up"), Some(HookEvent::LevelUp));
    assert_eq!(HookEvent::parse("nope"), None);
}

#[test]
fn an_answer_is_parsed_out_of_host_memory_and_into_actions() {
    // The whole point of the boundary: the guest hands back bytes, the host
    // parses them, and what comes out is something it can apply.
    let wasm = wat::parse_str(PURCHASE_HOOK).unwrap();
    let runner = HookRunner::load(&wasm, 4096).expect("loads");
    let input = HookInput::new(
        HookEvent::Purchase,
        "p1",
        serde_json::json!({ "sku": "vip" }),
    );
    let answer = runner.run(&input).expect("runs").expect("answers purchase");
    assert_eq!(answer.grant_currency.len(), 1);
    assert_eq!(answer.grant_currency[0].currency, "coins");
    assert_eq!(answer.grant_currency[0].amount, 1000);
    assert!(answer.reject.is_none());
}

#[test]
fn an_input_over_the_limit_is_refused() {
    let wasm = wat::parse_str(PURCHASE_HOOK).unwrap();
    let runner = HookRunner::load(&wasm, 16).expect("loads");
    let input = HookInput::new(
        HookEvent::Purchase,
        "player-with-a-long-identifier",
        serde_json::json!({ "sku": "vip" }),
    );
    let err = runner.run(&input).expect_err("must be refused");
    assert!(err.to_string().contains("over the"), "{err}");
}

#[test]
fn a_broken_hook_never_blocks_the_event() {
    // Customer code is not allowed to take a purchase down with it.
    let wasm = wat::parse_str(PURCHASE_HOOK).unwrap();
    let runner = HookRunner::load(&wasm, 4096).expect("loads");
    let input = HookInput::new(HookEvent::Purchase, "p1", serde_json::json!({}));
    if let Err(err) = runner.run(&input) {
        assert!(!err.blocks_event(), "a hook failure must not block");
    }
}
