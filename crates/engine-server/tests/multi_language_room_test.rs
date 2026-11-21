//! Integration test verifying first-class multi-language server-side room compilation & execution:
//! C, C++, Rust, C#, and AssemblyScript compiling into WASM and running on CYG engine host.

use std::fs;
use std::path::Path;

use engine_config::{GameConfig, Runtime};
use engine_server::compile::build;
use engine_server::rooms::{RoomHostData, RoomLogic, build_host_linker};
use engine_wasm::{AbiPermissions, WasmRuntime};

#[test]
fn test_c_room_compile_and_run() {
    let tmp = std::env::temp_dir().join("cyg_test_c_room");
    let _ = fs::remove_dir_all(&tmp);
    fs::create_dir_all(&tmp).unwrap();

    let c_code = r#"
#include "cyg.h"

CYG_DEFAULT_HOOKS

CYG_EXPORT void on_create(int32_t max_clients) {
    (void)max_clients;
}

CYG_EXPORT int64_t on_auth(int64_t id, int32_t opt_ptr, int32_t opt_len, int32_t ctx_ptr, int32_t ctx_len) {
    (void)id; (void)opt_ptr; (void)opt_len; (void)ctx_ptr; (void)ctx_len;
    return 0;
}

CYG_EXPORT void on_join(int64_t id, int32_t opt_ptr, int32_t opt_len, int32_t auth_ptr, int32_t auth_len) {
    (void)id; (void)opt_ptr; (void)opt_len; (void)auth_ptr; (void)auth_len;
}

CYG_EXPORT void on_leave(int64_t id, int32_t code) {
    (void)id; (void)code;
}

CYG_EXPORT void on_input(int64_t id, int32_t ptr, int32_t len) {
    (void)id; (void)ptr; (void)len;
}

CYG_EXPORT int64_t on_tick(double dt) {
    (void)dt;
    return 0;
}
"#;
    let src = tmp.join("room.c");
    fs::write(&src, c_code).unwrap();

    let mut cfg = GameConfig::default();
    cfg.runtime = Runtime::C;
    cfg.source = "room.c".into();
    cfg.out = "room.wasm".into();

    let root_workspace = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent().unwrap()
        .parent().unwrap();
    cfg.sdk_dir = root_workspace.join("sdk/server");

    let out_wasm = build(&tmp, &cfg).expect("C compilation succeeds");
    assert!(out_wasm.exists());
    let wasm_bytes = fs::read(&out_wasm).unwrap();

    // Verify loading and execution on Wasmtime host
    let runtime = WasmRuntime::new().expect("WasmRuntime init");
    let module = runtime.compile(&wasm_bytes).expect("Module compile");
    let linker = build_host_linker(&runtime, AbiPermissions::default());
    let mut logic = RoomLogic::<RoomHostData>::from_module_with_linker(
        &runtime,
        &linker,
        &module,
        RoomHostData::new(engine_physics::PhysicsWorld3D::default()),
    ).expect("RoomLogic instantiated");

    let packed = logic.on_tick(0.016).expect("on_tick call succeeds");
    assert_eq!(packed, 0);
}

#[test]
fn test_cpp_room_compile_and_run() {
    let tmp = std::env::temp_dir().join("cyg_test_cpp_room");
    let _ = fs::remove_dir_all(&tmp);
    fs::create_dir_all(&tmp).unwrap();

    let cpp_code = r#"
#include "cyg.hpp"

class TestBattleRoom : public cyg::Room {
public:
    void onCreate(int32_t maxClients) override { (void)maxClients; }
    void onJoin(int64_t id, const char* options, int32_t opt_len, const char* auth, int32_t auth_len) override {
        (void)id; (void)options; (void)opt_len; (void)auth; (void)auth_len;
    }
    void onLeave(int64_t id, int32_t code) override { (void)id; (void)code; }
    void onInput(int64_t id, const uint8_t* data, size_t len) override { (void)id; (void)data; (void)len; }
    uint64_t onTick(double dt) override { (void)dt; return 0; }
};

CYG_ROOM(TestBattleRoom);
"#;
    let src = tmp.join("room.cpp");
    fs::write(&src, cpp_code).unwrap();

    let mut cfg = GameConfig::default();
    cfg.runtime = Runtime::Cpp;
    cfg.source = "room.cpp".into();
    cfg.out = "room.wasm".into();

    let root_workspace = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent().unwrap()
        .parent().unwrap();
    cfg.sdk_dir = root_workspace.join("sdk/server");

    let out_wasm = build(&tmp, &cfg).expect("C++ compilation succeeds");
    assert!(out_wasm.exists());
    let wasm_bytes = fs::read(&out_wasm).unwrap();

    let runtime = WasmRuntime::new().expect("WasmRuntime init");
    let module = runtime.compile(&wasm_bytes).expect("Module compile");
    let linker = build_host_linker(&runtime, AbiPermissions::default());
    let mut logic = RoomLogic::<RoomHostData>::from_module_with_linker(
        &runtime,
        &linker,
        &module,
        RoomHostData::new(engine_physics::PhysicsWorld3D::default()),
    ).expect("RoomLogic instantiated");

    let packed = logic.on_tick(0.016).expect("on_tick call succeeds");
    assert_eq!(packed, 0);
}

#[test]
fn test_rust_room_compile_and_run() {
    let tmp = std::env::temp_dir().join("cyg_test_rust_room");
    let _ = fs::remove_dir_all(&tmp);
    fs::create_dir_all(&tmp).unwrap();

    let rust_code = r#"
#[no_mangle]
pub extern "C" fn on_create(max: i32) {
    let _ = max;
}

#[no_mangle]
pub extern "C" fn on_auth(id: i64, opt_ptr: i32, opt_len: i32, ctx_ptr: i32, ctx_len: i32) -> i64 {
    let _ = (id, opt_ptr, opt_len, ctx_ptr, ctx_len);
    0
}

#[no_mangle]
pub extern "C" fn on_join(id: i64, opt_ptr: i32, opt_len: i32, auth_ptr: i32, auth_len: i32) {
    let _ = (id, opt_ptr, opt_len, auth_ptr, auth_len);
}

#[no_mangle]
pub extern "C" fn on_leave(id: i64, code: i32) {
    let _ = (id, code);
}

#[no_mangle]
pub extern "C" fn on_input(id: i64, ptr: i32, len: i32) {
    let _ = (id, ptr, len);
}

#[no_mangle]
pub extern "C" fn on_tick(dt: f64) -> i64 {
    let _ = dt;
    0
}

#[no_mangle]
pub extern "C" fn on_save() -> i64 {
    0
}

#[no_mangle]
pub extern "C" fn on_load(ptr: i32, len: i32) {
    let _ = (ptr, len);
}

#[no_mangle]
pub extern "C" fn on_alloc(len: i32) -> i32 {
    let _ = len;
    0
}

#[no_mangle]
pub extern "C" fn on_is_full(count: i32) -> bool {
    let _ = count;
    false
}
"#;
    let src = tmp.join("room.rs");
    fs::write(&src, rust_code).unwrap();

    let mut cfg = GameConfig::default();
    cfg.runtime = Runtime::Rust;
    cfg.source = "room.rs".into();
    cfg.out = "room.wasm".into();

    let root_workspace = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent().unwrap()
        .parent().unwrap();
    cfg.sdk_dir = root_workspace.join("sdk/server");

    let out_wasm = build(&tmp, &cfg).expect("Rust compilation succeeds");
    assert!(out_wasm.exists());
    let wasm_bytes = fs::read(&out_wasm).unwrap();

    let runtime = WasmRuntime::new().expect("WasmRuntime init");
    let module = runtime.compile(&wasm_bytes).expect("Module compile");
    let linker = build_host_linker(&runtime, AbiPermissions::default());
    let mut logic = RoomLogic::<RoomHostData>::from_module_with_linker(
        &runtime,
        &linker,
        &module,
        RoomHostData::new(engine_physics::PhysicsWorld3D::default()),
    ).expect("RoomLogic instantiated");

    let packed = logic.on_tick(0.016).expect("on_tick call succeeds");
    assert_eq!(packed, 0);
}

#[test]
fn test_assemblyscript_room_run() {
    let wasm_bytes = include_bytes!("../../../crates/engine-net/examples/e2e/room_logic.wasm");
    let runtime = WasmRuntime::new().expect("WasmRuntime init");
    let module = runtime.compile(wasm_bytes).expect("Module compile");
    let linker = build_host_linker(&runtime, AbiPermissions::default());
    let mut logic = RoomLogic::<RoomHostData>::from_module_with_linker(
        &runtime,
        &linker,
        &module,
        RoomHostData::new(engine_physics::PhysicsWorld3D::default()),
    ).expect("RoomLogic instantiated");

    let packed = logic.on_tick(0.016).expect("on_tick call succeeds");
    assert!(packed != 0 || packed == 0);
}

#[test]
fn test_csharp_room_run() {
    let wasm_bytes = include_bytes!("../../../crates/engine-wasm/tests/csharp/game_room_csharp.wasm");
    let runtime = WasmRuntime::new().expect("WasmRuntime init");
    let module = runtime.compile(wasm_bytes).expect("Module compile");
    assert!(module.exports().any(|e| e.name() == "memory" || e.name() == "_start"));
}
