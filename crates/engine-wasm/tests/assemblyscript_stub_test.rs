use std::time::Instant;
use engine_wasm::{WasmRuntime, wasm_module};

// Declare the typed wrapper matching game_room.ts exports
wasm_module! {
    pub struct AsGameRoom {
        fn init();
        fn update(dt: f32, speed_x: f32, speed_y: f32);
        fn getScore() -> i32;
        fn getPositionX() -> f32;
        fn getPositionY() -> f32;
        fn calculateDamage(base: i32, multiplier: f32) -> i32;
    }
}

#[test]
fn test_real_assemblyscript_with_runtime_stub() {
    println!("\n=======================================================");
    println!(" REAL ASSEMBLYSCRIPT (--runtime stub) BENCHMARK TEST   ");
    println!("=======================================================");

    let runtime = WasmRuntime::new().expect("Runtime init ok");

    // Load the 239-byte binary compiled directly from TypeScript/AssemblyScript
    let wasm_bytes = include_bytes!("assemblyscript/game_room.wasm");
    println!("1. AssemblyScript WASM Binary Size:  {} bytes!", wasm_bytes.len());

    let module = runtime.compile(wasm_bytes).expect("WASM Compilation ok");

    // Instantiate with our type-safe macro
    let mut room = AsGameRoom::new(&runtime, &module).expect("Room instantiation ok");

    // Test 1: Initialize game room
    room.init().expect("init ok");
    assert_eq!(room.getPositionX().unwrap(), 100.0);
    assert_eq!(room.getPositionY().unwrap(), 100.0);
    assert_eq!(room.getScore().unwrap(), 0);
    println!("✓ State initialized: Pos(100.0, 100.0), Score(0)");

    // Test 2: Update tick simulation (dt = 0.016, speedX = 100.0, speedY = 50.0)
    room.update(0.016, 100.0, 50.0).expect("update ok");
    let new_x = room.getPositionX().unwrap();
    let new_y = room.getPositionY().unwrap();
    let new_score = room.getScore().unwrap();

    println!("✓ After 1 tick update: Pos({:.2}, {:.2}), Score({})", new_x, new_y, new_score);
    assert!((new_x - 101.6).abs() < 0.01);
    assert!((new_y - 100.8).abs() < 0.01);
    assert_eq!(new_score, 1);

    // Test 3: Math function
    let dmg = room.calculateDamage(50, 1.5).expect("calculateDamage ok");
    assert_eq!(dmg, 75);
    println!("✓ calculateDamage(50, 1.5) = {}", dmg);

    // Test 4: High-frequency 1,000,000 tick benchmark
    let iterations = 1_000_000;
    let start_bench = Instant::now();

    for _ in 0..iterations {
        room.update(0.016, 10.0, 20.0).unwrap();
    }

    let elapsed = start_bench.elapsed();
    let ns_per_tick = (elapsed.as_nanos() as f64) / (iterations as f64);
    let ticks_per_sec = (iterations as f64) / elapsed.as_secs_f64();

    println!("\n--- [AssemblyScript --runtime stub Speed Results] ---");
    println!("Total Time for 1,000,000 Ticks: {:?}", elapsed);
    println!("Average Latency per Tick:       {:.2} nanoseconds", ns_per_tick);
    println!("Max Throughput (Single Thread): {:.0} ticks/second", ticks_per_sec);
    println!("Final Score after 1M Ticks:     {}", room.getScore().unwrap());
    println!("=======================================================\n");

    assert!(ns_per_tick < 30.0, "AssemblyScript with runtime stub must be under 30ns");
}
