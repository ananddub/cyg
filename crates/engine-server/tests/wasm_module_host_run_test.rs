//! The half a console WASM upload cannot show from its own side.
//!
//! `engine-api`'s `gap_console.rs` proves the upload path: the ABI gate's verdict,
//! the row's digest, the stored bytes byte for byte and the hot-swap request the
//! control plane publishes. It stops where the control plane stops — it never
//! loads a Cranelift compiler. This test picks up at the sim plane and runs the
//! *same file* through what `engine-server/src/host.rs` does at boot and on
//! reload: compile, link against the real game-host imports, instantiate, tick.

use engine_api::services::wasm::{REQUIRED_EXPORTS, WasmService};
use engine_server::rooms::{RoomHostData, RoomLogic, build_host_linker};
use engine_wasm::{AbiPermissions, WasmRuntime};

/// The guest `gap_console.rs` uploads through
/// `POST /v2/console/games/{gameId}/wasm`, whose stored bytes that suite compares
/// against this file byte for byte.
const ROOM_MODULE: &[u8] = include_bytes!("../../engine-net/examples/e2e/room_logic.wasm");

/// Unpack a guest tick return the way `netcode.rs` does: `(ptr << 32) | len`.
fn unpack(packed: i64) -> (u32, u32) {
    (((packed >> 32) as u32), packed as u32)
}

/// The delta buffer this guest writes, per its own serialization comment:
/// `[u32 player_count]`, then per player `[i64 id, f64 x, f64 y, f64 score, f64 hp]`.
fn decode_players(blob: &[u8]) -> Vec<(i64, f64, f64, f64, f64)> {
    let count = u32::from_le_bytes(blob[0..4].try_into().expect("a player count")) as usize;
    (0..count)
        .map(|index| {
            let at = 4 + index * 40;
            let field = |offset: usize| {
                f64::from_le_bytes(
                    blob[at + offset..at + offset + 8]
                        .try_into()
                        .expect("one 8-byte field"),
                )
            };
            (
                i64::from_le_bytes(blob[at..at + 8].try_into().expect("a player id")),
                field(8),
                field(16),
                field(24),
                field(32),
            )
        })
        .collect()
}

#[test]
fn a_console_accepted_module_compiles_links_and_runs_in_the_host() {
    // ---- The control plane's own verdict on these bytes ----
    let gate = WasmService::validate_binary(ROOM_MODULE);
    assert!(
        gate.valid,
        "the console gate accepts its own test module: {gate:?}"
    );
    assert!(gate.exports_missing.is_empty(), "{gate:?}");
    for name in REQUIRED_EXPORTS {
        assert!(
            gate.exports_found.iter().any(|found| found == name),
            "{name} is one of the exports the host binds"
        );
    }

    // ---- The sim plane: compile, the real host linker, instantiate ----
    let runtime = WasmRuntime::new().expect("a wasmtime runtime");
    let module = runtime
        .compile(ROOM_MODULE)
        .expect("Cranelift compiles the bytes the gate validated");
    let linker = build_host_linker(&runtime, AbiPermissions::default());
    let mut logic = RoomLogic::<RoomHostData>::from_module_with_linker(
        &runtime,
        &linker,
        &module,
        RoomHostData::new(engine_physics::PhysicsWorld3D::default()),
    )
    .expect("every host import resolves and all eight ABI exports bind");

    // ---- And then it actually simulates a frame ----
    logic.on_create(16).expect("guest on_create");
    logic.on_join(7, 100.0, 100.0).expect("guest on_join");

    let (ptr, len) = unpack(logic.on_tick(0.05).expect("guest on_tick"));
    let blob = logic
        .read_memory(ptr, len)
        .expect("the guest's delta buffer is in its linear memory");
    assert_eq!(
        decode_players(&blob),
        vec![(7, 100.0, 100.0, 0.0, 100.0)],
        "the join landed in the state the guest encodes"
    );

    // The input path the host drives on every message: the guest allocates a
    // buffer, the host writes the payload into it, `on_input` reads it back.
    let input = logic
        .on_alloc(8)
        .expect("the guest allocator hands back a buffer");
    // flags = 2 (`S`), then a little-endian f32 aim angle.
    logic
        .write_memory(input as u32, &[2, 0, 0, 0, 0, 0, 0, 0])
        .expect("write the input payload");
    logic
        .on_input(7, input, 8)
        .expect("guest on_input reads what the host wrote");

    let (ptr, len) = unpack(logic.on_tick(0.05).expect("the second on_tick"));
    let blob = logic
        .read_memory(ptr, len)
        .expect("the second delta buffer");
    let moved = decode_players(&blob);
    assert_eq!(moved.len(), 1, "{moved:?}");
    // The guest's own speed is 150 units/sec, so one 0.05 s tick moves 7.5.
    assert!(
        (moved[0].2 - 107.5).abs() < 1e-9,
        "the input moved the player through the guest's simulation: {moved:?}"
    );

    // The remaining ABI entries, all called through the host's handles.
    logic.on_leave(7).expect("guest on_leave");
    let (ptr, len) = unpack(logic.on_tick(0.05).expect("on_tick after the leave"));
    assert!(
        decode_players(&logic.read_memory(ptr, len).expect("the third delta")).is_empty(),
        "the leaving player left the encoded state"
    );
    let saved = logic.on_save().expect("guest on_save");
    logic.on_load(saved as i32, 0).expect("guest on_load");
}
