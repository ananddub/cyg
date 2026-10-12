use engine_wasm::{WasmConfig, WasmRuntime};

const MODULE: &str = r#"
(module
    (memory (export "memory") 1)
    (global $rooms i32 (i32.const 0))
    (func (export "calculate_damage") (param $base i32) (param $mult f32) (result i32)
        (i32.trunc_f32_s
            (f32.mul (f32.convert_i32_s (local.get $base)) (local.get $mult))
        )
    )
)
"#;

fn compile(runtime: &WasmRuntime) -> wasmtime::Module {
    let bytes = wat::parse_str(MODULE).expect("WAT parse ok");
    runtime.compile(&bytes).expect("Compile ok")
}

#[test]
fn test_prepare_and_instantiate_multiple_rooms() {
    let runtime = WasmRuntime::new().expect("Runtime should initialize");
    let module = compile(&runtime);

    let pre = runtime
        .prepare_standalone::<()>(&module)
        .expect("Prepare ok");

    // Spawn several rooms from the same pre-instantiated template.
    let mut rooms = Vec::new();
    for _ in 0..8 {
        rooms.push(pre.instantiate(()).expect("Instantiate ok"));
    }

    for room in &mut rooms {
        let damage: i32 = room
            .call("calculate_damage", (10i32, 2.5f32))
            .expect("Call ok");
        assert_eq!(damage, 25);
    }
}

#[test]
fn test_pooling_allocator_runtime() {
    let runtime = WasmRuntime::new_with_config(WasmConfig {
        use_pooling_allocator: true,
        ..WasmConfig::default()
    })
    .expect("Pooling runtime should initialize");
    assert!(runtime.config().use_pooling_allocator);

    let module = compile(&runtime);
    let pre = runtime
        .prepare_standalone::<()>(&module)
        .expect("Prepare ok");

    let mut room = pre.instantiate(()).expect("Instantiate ok");
    let damage: i32 = room
        .call("calculate_damage", (7i32, 3.0f32))
        .expect("Call ok");
    assert_eq!(damage, 21);
}

#[test]
fn test_serialize_deserialize_module_roundtrip() {
    let runtime = WasmRuntime::new().expect("Runtime should initialize");
    let module = compile(&runtime);

    let path = std::env::temp_dir().join("engine_wasm_test_module.cwasm");
    runtime
        .serialize_module(&module, &path)
        .expect("Serialize ok");

    // SAFETY: file was just written by serialize_module in this test.
    let restored = unsafe { runtime.deserialize_module(&path) }.expect("Deserialize ok");

    let mut room = runtime
        .instantiate_standalone(&restored, ())
        .expect("Instantiate ok");
    let damage: i32 = room
        .call("calculate_damage", (4i32, 5.0f32))
        .expect("Call ok");
    assert_eq!(damage, 20);

    std::fs::remove_file(&path).ok();
}
