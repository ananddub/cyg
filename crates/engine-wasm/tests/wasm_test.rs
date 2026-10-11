use engine_wasm::WasmRuntime;

#[test]
fn test_generic_wasm_runtime_execution() {
    let runtime = WasmRuntime::new().expect("Runtime should initialize");

    // Any arbitrary WASM module without any game event constraints
    let wat = r#"
    (module
        (memory (export "memory") 1)
        (func (export "calculate_damage") (param $base_dmg i32) (param $multiplier f32) (result i32)
            (i32.trunc_f32_s
                (f32.mul (f32.convert_i32_s (local.get $base_dmg)) (local.get $multiplier))
            )
        )
    )
    "#;

    let wasm_bytes = wat::parse_str(wat).expect("WAT parse ok");
    let module = runtime.compile(&wasm_bytes).expect("Compile ok");

    // Instantiate standalone with arbitrary host state (e.g. empty tuple or any struct)
    let mut instance = runtime
        .instantiate_standalone(&module, ())
        .expect("Instantiate ok");

    // Generic function call: call any function with any signature!
    let damage: i32 = instance
        .call("calculate_damage", (50i32, 1.5f32))
        .expect("Call ok");

    assert_eq!(damage, 75);
}

#[test]
fn test_generic_memory_read_and_write() {
    let runtime = WasmRuntime::new().expect("Runtime should initialize");

    let wat = r#"
    (module
        (memory (export "memory") 1)
    )
    "#;

    let wasm_bytes = wat::parse_str(wat).expect("WAT parse ok");
    let module = runtime.compile(&wasm_bytes).expect("Compile ok");
    let mut instance = runtime
        .instantiate_standalone(&module, ())
        .expect("Instantiate ok");

    // Write bytes to linear memory at offset 128
    let secret = b"Pure WASM Runtime Data";
    instance.write_memory(128, secret).expect("Write ok");

    // Read bytes back from linear memory
    let read_back = instance
        .read_memory(128, secret.len() as u32)
        .expect("Read ok");
    assert_eq!(read_back, secret);
}
