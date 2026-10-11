use engine_wasm::{WasmRuntime, wasm_module};

// 1. Declare the type-safe wrapper using wasm_module!
wasm_module! {
    pub struct GameRoom {
        fn calculate_damage(base_dmg: i32, multiplier: f32) -> i32;
        fn on_tick(dt: f32, tick: u64);
        fn get_score() -> i32;
        fn ping();
    }
}

#[test]
fn test_wasm_module_macro_seamless_calling() {
    let runtime = WasmRuntime::new().expect("Runtime should initialize");

    // Sample WASM module implementing the functions
    let wat = r#"
    (module
        (global $score (mut i32) (i32.const 100))

        ;; calculate_damage(base_dmg, multiplier) -> i32
        (func (export "calculate_damage") (param $base_dmg i32) (param $multiplier f32) (result i32)
            (i32.trunc_f32_s
                (f32.mul (f32.convert_i32_s (local.get $base_dmg)) (local.get $multiplier))
            )
        )

        ;; on_tick(dt, tick)
        (func (export "on_tick") (param $dt f32) (param $tick i64)
            (global.set $score (i32.add (global.get $score) (i32.const 10)))
        )

        ;; get_score() -> i32
        (func (export "get_score") (result i32)
            (global.get $score)
        )

        ;; ping()
        (func (export "ping"))
    )
    "#;

    let wasm_bytes = wat::parse_str(wat).expect("WAT parse ok");
    let module = runtime.compile(&wasm_bytes).expect("Compile ok");

    // 2. Instantiate using the generated struct directly!
    let mut room = GameRoom::new(&runtime, &module).expect("Room instantiation ok");

    // 3. Look at the calling code - it looks 100% like a normal Rust struct!
    // No .call(), no string function names, no tuple wrapping!

    // Call 0-arg function
    room.ping().expect("ping should succeed");

    // Call 2-args function with return value
    let dmg = room.calculate_damage(60, 2.5).expect("calculate_damage should succeed");
    assert_eq!(dmg, 150);

    // Call getter
    let initial_score = room.get_score().expect("get_score should succeed");
    assert_eq!(initial_score, 100);

    // Call tick (mutates in-WASM score)
    room.on_tick(0.016, 1).expect("on_tick should succeed");

    let updated_score = room.get_score().expect("get_score should succeed");
    assert_eq!(updated_score, 110);
}
