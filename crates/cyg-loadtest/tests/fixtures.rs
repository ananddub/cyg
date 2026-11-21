//! Fixture-based decode check: run the REAL captured server deltas
//! (crates/cyg-core-rs/tests/fixtures) through the loadtest's frame
//! classifier + `ClientCore::apply_payload`, exactly the path the CLI uses,
//! and assert the core reaches Ready.

use std::path::Path;

use cyg_core_rs::ClientCore;
use cyg_loadtest::{classify_frame, delta_kind, Inbound};

fn fixtures_dir() -> std::path::PathBuf {
    // CARGO_MANIFEST_DIR = crates/cyg-loadtest
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../cyg-core-rs/tests/fixtures")
}

fn read_u32_lp_frames(bytes: &[u8]) -> Vec<&[u8]> {
    let mut frames = Vec::new();
    let mut at = 0usize;
    while at + 4 <= bytes.len() {
        let len = u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap()) as usize;
        let start = at + 4;
        let end = start + len;
        if end > bytes.len() {
            break;
        }
        frames.push(&bytes[start..end]);
        at = end;
    }
    frames
}

#[test]
fn captured_deltas_reach_ready_through_loadtest_path() {
    let dir = fixtures_dir();
    let descriptor = std::fs::read(dir.join("descriptor.bin")).expect("descriptor fixture");
    let payloads = std::fs::read(dir.join("payloads.bin")).expect("payload fixture");

    assert_eq!(classify_frame(&descriptor), Inbound::Descriptor);
    let mut core = ClientCore::new(&descriptor).expect("descriptor parses");

    let mut fulls = 0u64;
    let mut patches = 0u64;
    for frame in read_u32_lp_frames(&payloads) {
        if classify_frame(frame) != Inbound::Delta {
            continue;
        }
        if delta_kind(frame).map(cyg_loadtest::is_full_kind).unwrap_or(false) {
            fulls += 1;
        } else if delta_kind(frame).map(cyg_loadtest::is_patch_kind).unwrap_or(false) {
            patches += 1;
        }
        core.apply_payload(frame).expect("frame decodes");
    }
    assert!(core.state().ready, "fixture deltas never reached Ready");
    assert!(fulls >= 1, "expected at least one full snapshot");
    assert!(entity_count(&core) > 0, "expected decoded entities");
}

fn entity_count(core: &ClientCore) -> usize {
    (0..core.collection_count())
        .filter_map(|i| core.collection_at(i))
        .map(|c| c.entities.len())
        .sum()
}

#[test]
fn input_encoding_matches_input_schema() {
    let dir = fixtures_dir();
    let descriptor = std::fs::read(dir.join("descriptor.bin")).expect("descriptor fixture");
    let core = ClientCore::new(&descriptor).expect("descriptor parses");
    let input = core
        .descriptor()
        .schemas
        .iter()
        .find(|s| s.name == cyg_core_rs::INPUT_SCHEMA_NAME)
        .expect("descriptor has an Input schema");
    let values = cyg_loadtest::movement_values(&input.fields, 0).expect("movement values");
    let frame = cyg_loadtest::encode_schema_input_with(&core, 7, &values).expect("input encodes");
    // [0x05][7]["__input"][seq u16 LE][fields...] — moveX f32 + jump bool
    assert_eq!(&frame[..9], b"\x05\x07__input");
    assert_eq!(&frame[9..11], &7u16.to_le_bytes());
    let body: usize = input
        .fields
        .iter()
        .map(|f| match f.kind {
            cyg_core_rs::WireKind::Bool | cyg_core_rs::WireKind::U8 | cyg_core_rs::WireKind::I8 => 1,
            cyg_core_rs::WireKind::F32 | cyg_core_rs::WireKind::U32 | cyg_core_rs::WireKind::I32 => 4,
            _ => 8,
        })
        .sum();
    assert_eq!(frame.len(), 9 + 2 + body);
}
