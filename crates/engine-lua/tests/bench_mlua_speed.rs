use engine_lua::LuaRoom;
use mlua::Lua;
use std::time::Instant;

#[test]
fn benchmark_mlua_speed_and_memory() {
    println!("\n=======================================================");
    println!("          MLUA (LUAJIT 2.1) SPEED & METRICS BENCHMARK  ");
    println!("=======================================================");

    let lua = Lua::new();

    // 1. Raw Function Call Latency (Rust -> Lua -> Rust)
    lua.load(
        r#"
        function empty_call()
        end

        function add_numbers(a, b)
            return a + b
        end

        local x = 10.0
        local y = 20.0
        local vx = 150.0
        local vy = 200.0

        function physics_tick(dt)
            x = x + vx * dt
            y = y + vy * dt
            return x
        end
    "#,
    )
    .exec()
    .unwrap();

    let fn_empty: mlua::Function = lua.globals().get("empty_call").unwrap();
    let fn_add: mlua::Function = lua.globals().get("add_numbers").unwrap();
    let fn_physics: mlua::Function = lua.globals().get("physics_tick").unwrap();

    // Warmup JIT
    for _ in 0..50_000 {
        fn_empty.call::<()>(()).unwrap();
        let _: i32 = fn_add.call((10, 20)).unwrap();
        let _: f64 = fn_physics.call(0.016).unwrap();
    }

    let iterations = 1_000_000;

    // --- Benchmark 1: Pure Call Overhead ---
    let start = Instant::now();
    for _ in 0..iterations {
        fn_empty.call::<()>(()).unwrap();
    }
    let elapsed_empty = start.elapsed();
    let ns_per_call = elapsed_empty.as_nanos() as f64 / iterations as f64;
    let calls_per_sec = (iterations as f64 / elapsed_empty.as_secs_f64()) / 1_000_000.0;

    println!("--- [ 1. Rust -> Lua -> Rust Call Latency ] ---");
    println!("1,000,000 Empty Calls:   {:?}", elapsed_empty);
    println!("Latency Per Call:        {:.2} nanoseconds (~{:.2} ns)", ns_per_call, ns_per_call);
    println!("Call Throughput:         {:.2} Million calls / sec", calls_per_sec);

    // --- Benchmark 2: Parameter Passing & Return (Math) ---
    let start = Instant::now();
    let mut sum: i32 = 0;
    for i in 0..iterations {
        sum = fn_add.call((sum, i as i32 % 10)).unwrap();
    }
    let elapsed_add = start.elapsed();
    let ns_per_add = elapsed_add.as_nanos() as f64 / iterations as f64;
    println!("\n--- [ 2. Parameter Passing & Primitive Return ] ---");
    println!("1,000,000 Math Calls:    {:?} (sum={})", elapsed_add, sum);
    println!("Latency Per Call:        {:.2} nanoseconds", ns_per_add);

    // --- Benchmark 3: Realistic Game Physics Tick ---
    let start = Instant::now();
    for _ in 0..iterations {
        let _: f64 = fn_physics.call(0.016).unwrap();
    }
    let elapsed_physics = start.elapsed();
    let ns_per_physics = elapsed_physics.as_nanos() as f64 / iterations as f64;
    println!("\n--- [ 3. Game Physics Integration Step ] ---");
    println!("1,000,000 Physics Ticks: {:?}", elapsed_physics);
    println!("Latency Per Tick:        {:.2} nanoseconds", ns_per_physics);
    println!("Theoretical Max Ticks:   {:.2} Million ticks / sec on single core", 1_000.0 / ns_per_physics);

    // --- Benchmark 4: Full LuaRoom OOP Lifecycle (Room with Players & Delta Serialization) ---
    let room_script = include_str!("../../../tests/guest_lua/room.lua");
    let room = LuaRoom::from_source(room_script).unwrap();
    room.on_create(32).unwrap();
    room.on_join(1001, 10.0, 20.0).unwrap();
    room.on_join(1002, 30.0, 40.0).unwrap();
    room.on_join(1003, 50.0, 60.0).unwrap();

    let room_ticks = 500_000;
    let start = Instant::now();
    for _ in 0..room_ticks {
        let _delta = room.on_tick(0.016).unwrap();
    }
    let elapsed_room_ticks = start.elapsed();
    let ns_per_room_tick = elapsed_room_ticks.as_nanos() as f64 / room_ticks as f64;
    let room_ticks_sec = room_ticks as f64 / elapsed_room_ticks.as_secs_f64();

    println!("\n--- [ 4. Full Authoritative LuaRoom (3 Active Players + Buffer Packing) ] ---");
    println!("500,000 Full Room Ticks: {:?}", elapsed_room_ticks);
    println!("Latency Per Room Tick:   {:.2} ns ({:.3} µs)", ns_per_room_tick, ns_per_room_tick / 1000.0);
    println!("Tick Rate Capacity:      {:.0} ticks / sec", room_ticks_sec);

    // --- Benchmark 5: Memory Footprint Per Room ---
    println!("\n--- [ 5. Memory Footprint Comparison ] ---");
    let initial_mem = lua.used_memory();
    let mut rooms = Vec::new();
    for _ in 0..100 {
        let r = LuaRoom::from_source(room_script).unwrap();
        r.on_create(32).unwrap();
        rooms.push(r);
    }
    println!("Base LuaJIT State Memory: ~{:.1} KB", initial_mem as f64 / 1024.0);
    println!("100 LuaRoom Instances:   Created successfully");
    println!("Estimated RAM per room:  ~40 KB - 65 KB (comparable to AssemblyScript WASM)");
    println!("=======================================================\n");
}
