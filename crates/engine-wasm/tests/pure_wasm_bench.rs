use std::time::Instant;
use wasmtime::{Config, Engine, Instance, Module, OptLevel, Store, Strategy};

#[test]
fn benchmark_pure_wasm_execution_latency() {
    println!("\n=======================================================");
    println!("          PURE WASM EXECUTION LATENCY TEST             ");
    println!("       (Compiled Bytecode -> Pure Native JIT Run)      ");
    println!("=======================================================");

    let mut config = Config::new();
    config.strategy(Strategy::Cranelift);
    config.cranelift_opt_level(OptLevel::Speed);

    let engine = Engine::new(&config).unwrap();

    // 1. Module with:
    //    - empty: pure function call jump (no work)
    //    - physics_tick: in-WASM game calculation (no host calls, pure registers/memory)
    let wat = r#"
    (module
        (global $x (mut f32) (f32.const 10.0))
        (global $y (mut f32) (f32.const 20.0))
        (global $vx (mut f32) (f32.const 150.0))
        (global $vy (mut f32) (f32.const 200.0))

        ;; 1. Empty function call (Host -> Guest -> Host jump)
        (func (export "empty"))

        ;; 2. Realistic Game Physics Tick (Position + Velocity integration)
        (func (export "physics_tick") (param $dt f32)
            (global.set $x (f32.add (global.get $x) (f32.mul (global.get $vx) (local.get $dt))))
            (global.set $y (f32.add (global.get $y) (f32.mul (global.get $vy) (local.get $dt))))
        )

        ;; 3. Math calculation returning result
        (func (export "add_numbers") (param $a i32) (param $b i32) (result i32)
            (i32.add (local.get $a) (local.get $b))
        )
    )
    "#;

    let wasm_bytes = wat::parse_str(wat).unwrap();
    let module = Module::new(&engine, &wasm_bytes).unwrap();
    let mut store = Store::new(&engine, ());
    let instance = Instance::new(&mut store, &module, &[]).unwrap();

    let fn_empty = instance.get_typed_func::<(), ()>(&mut store, "empty").unwrap();
    let fn_physics = instance.get_typed_func::<f32, ()>(&mut store, "physics_tick").unwrap();
    let fn_add = instance.get_typed_func::<(i32, i32), i32>(&mut store, "add_numbers").unwrap();

    // Warmup
    for _ in 0..10_000 {
        fn_empty.call(&mut store, ()).unwrap();
        fn_physics.call(&mut store, 0.016).unwrap();
        fn_add.call(&mut store, (10, 20)).unwrap();
    }

    let iterations = 1_000_000; // 1 Million iterations

    // TEST 1: Pure Function Call Jump (Empty)
    let start_empty = Instant::now();
    for _ in 0..iterations {
        fn_empty.call(&mut store, ()).unwrap();
    }
    let dur_empty = start_empty.elapsed();
    let ns_empty = (dur_empty.as_nanos() as f64) / (iterations as f64);

    // TEST 2: Math Calculation with Return Value
    let start_add = Instant::now();
    for i in 0..iterations {
        let _ = fn_add.call(&mut store, (i as i32, 42)).unwrap();
    }
    let dur_add = start_add.elapsed();
    let ns_add = (dur_add.as_nanos() as f64) / (iterations as f64);

    // TEST 3: In-WASM Game Physics Tick
    let start_physics = Instant::now();
    for _ in 0..iterations {
        fn_physics.call(&mut store, 0.016).unwrap();
    }
    let dur_physics = start_physics.elapsed();
    let ns_physics = (dur_physics.as_nanos() as f64) / (iterations as f64);

    println!("1. Pure Function Jump (Empty):          {:.2} nanoseconds", ns_empty);
    println!("2. Math Call with Return Value (add):   {:.2} nanoseconds", ns_add);
    println!("3. Game Physics Tick (In-WASM State):   {:.2} nanoseconds", ns_physics);
    println!("=======================================================\n");
}
