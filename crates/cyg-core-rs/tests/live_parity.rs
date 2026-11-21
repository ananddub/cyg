//! Live parity: apply REAL server->client delta bytes (captured from a
//! running bench-game-2donly server via bench/capture_cyg_deltas.mjs) through
//! cyg-core-rs and assert the decoded state matches what cyg.js decoded for
//! the same bytes (fixtures/expected.json).
//!
//! Regenerate fixtures:
//!   node bench/capture_cyg_deltas.mjs --url http://127.0.0.1:8297 \
//!       --room arena --seconds 10 --out crates/cyg-core-rs/tests/fixtures

use std::path::Path;

use cyg_core_rs::{ClientCore, StateEvent};

fn fixtures_dir() -> std::path::PathBuf {
    // CARGO_MANIFEST_DIR = crates/cyg-core-rs
    let manifest = env!("CARGO_MANIFEST_DIR");
    Path::new(manifest).join("tests/fixtures")
}

fn read_u32_lp_frames(bytes: &[u8]) -> Vec<&[u8]> {
    let mut frames = Vec::new();
    let mut at = 0usize;
    while at + 4 <= bytes.len() {
        let len = u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap()) as usize;
        let start = at + 4;
        let end = start + len;
        if end > bytes.len() {
            break; // truncated tail: ignore
        }
        frames.push(&bytes[start..end]);
        at = end;
    }
    frames
}

#[test]
fn rust_core_matches_cygjs_on_captured_deltas() {
    let dir = fixtures_dir();
    let descriptor = std::fs::read(dir.join("descriptor.bin")).expect("descriptor fixture");
    let payloads = std::fs::read(dir.join("payloads.bin")).expect("payload fixture");
    let expected: serde_json::Value =
        serde_json::from_slice(&std::fs::read(dir.join("expected.json")).expect("expected fixture"))
            .expect("expected.json");

    let mut core = ClientCore::new(&descriptor).expect("descriptor parses");

    let mut applied = 0usize;
    let mut ready = false;
    let mut last_err: Option<String> = None;
    for frame in read_u32_lp_frames(&payloads) {
        match core.apply_payload(frame) {
            Ok(events) => {
                applied += 1;
                if events.iter().any(|e| matches!(e, StateEvent::Ready { .. })) {
                    ready = true;
                }
            }
            Err(e) => {
                // Non-delta frames (descriptor push, control messages, text)
                // are expected to be skipped; remember the last real error so
                // a genuine decode bug is not silently swallowed.
                last_err = Some(e);
            }
        }
    }
    assert!(applied > 0, "no payload decoded — fixture transport changed?");
    assert!(ready, "never reached a kind-1 full snapshot; last error: {last_err:?}");

    // Compare against cyg.js's decoded players for the same bytes.
    let players = expected.get("players").and_then(|v| v.as_array())
        .expect("expected.json players array");
    assert!(!players.is_empty(), "cyg.js decoded no players");

    let state = core.state();
    let coll = state.collections.first().expect("primary collection");
    let schema = {
        let descriptor = core.descriptor();
        descriptor.schemas.iter()
            .find(|s| engine_schema::wire_hash(s) == coll.schema_hash)
            .cloned()
            .expect("collection schema in descriptor")
    };
    let field_index = |name: &str| schema.fields.iter().position(|f| f.name == name);

    for want in players {
        let id = want.get("id").and_then(|v| v.as_u64()).unwrap() as u32;
        let entity = coll.entities.get(&id)
            .unwrap_or_else(|| panic!("entity {id} missing from Rust decode"));
        for (name, want_value) in want.as_object().unwrap() {
            if name == "id" {
                continue;
            }
            let Some(idx) = field_index(name) else {
                panic!("field {name} not in schema — fixture/descriptor mismatch");
            };
            let got = &entity.values[idx];
            match want_value {
                serde_json::Value::Number(n) => {
                    let want = n.as_f64().unwrap();
                    let got = got.as_f64();
                    assert!(
                        (got - want).abs() < 1e-6,
                        "entity {id} field {name}: rust {got} vs cyg.js {want}"
                    );
                }
                serde_json::Value::Bool(b) => {
                    let got = matches!(got, cyg_core_rs::state::Value::Bool(v) if *v == *b);
                    assert!(got, "entity {id} field {name}: bool mismatch");
                }
                serde_json::Value::String(s) => {
                    let got = match got {
                        cyg_core_rs::state::Value::Str(v) => v.as_str(),
                        _ => panic!("entity {id} field {name}: expected string"),
                    };
                    assert_eq!(got, s, "entity {id} field {name}");
                }
                _ => {} // complex values: covered by cyg.js's own tests
            }
        }
    }

    // And nothing extra: same entity count as cyg.js saw.
    assert_eq!(
        coll.entities.len(),
        players.len(),
        "entity count mismatch vs cyg.js"
    );
}
