use engine_wasm::{WasmConfig, WasmRuntime};
use std::time::Instant;

#[test]
fn test_csharp_wasm_compilation_and_size_benchmark() {
    println!("\n=======================================================");
    println!("     C# (.NET WASM) vs ASSEMBLYSCRIPT BENCHMARK        ");
    println!("=======================================================");

    // 1. Binary Size Comparison
    let as_bytes = include_bytes!("assemblyscript/game_room.wasm");
    let cs_bytes = include_bytes!("csharp/game_room_csharp.wasm");

    println!("--- [ 1. Binary Footprint Comparison ] ---");
    println!(
        "AssemblyScript Binary Size: {:>10} bytes ({:.2} KB)",
        as_bytes.len(),
        as_bytes.len() as f64 / 1024.0
    );
    println!(
        "C# (.NET) Binary Size:      {:>10} bytes ({:.2} MB)",
        cs_bytes.len(),
        cs_bytes.len() as f64 / (1024.0 * 1024.0)
    );
    println!(
        "Size Ratio (C# / AS):       {:.1}x LARGER!",
        cs_bytes.len() as f64 / as_bytes.len() as f64
    );

    // 2. Cranelift JIT Compilation Benchmark
    let config = WasmConfig {
        optimize_for_speed: true,
        enable_epoch_interruption: false,
        epoch_deadline_ticks: None,
        use_pooling_allocator: false, // Standard allocator for compiling big modules
    };
    let runtime = WasmRuntime::new_with_config(config).expect("Runtime initialization ok");

    println!("\n--- [ 2. Cranelift Compilation Time ] ---");
    // Compile AssemblyScript
    let as_start = Instant::now();
    let as_module = runtime.compile(as_bytes).expect("AS compilation ok");
    let as_compile_time = as_start.elapsed();
    println!("AssemblyScript compile time: {:?}", as_compile_time);

    // Compile C#
    let cs_start = Instant::now();
    let cs_module = runtime.compile(cs_bytes).expect("C# compilation ok");
    let cs_compile_time = cs_start.elapsed();
    println!("C# (.NET) compile time:      {:?}", cs_compile_time);
    println!(
        "Compilation Slowdown:        {:.1}x slower to compile!",
        cs_compile_time.as_secs_f64() / as_compile_time.as_secs_f64()
    );

    // 3. Inspect Imports & Exports
    println!("\n--- [ 3. WASM Imports & Dependencies ] ---");
    let as_imports: Vec<_> = as_module.imports().collect();
    let cs_imports: Vec<_> = cs_module.imports().collect();
    println!("AssemblyScript Host Imports: {}", as_imports.len());
    println!(
        "C# (.NET) Host Imports:      {} (requires POSIX/WASI OS subsystem)",
        cs_imports.len()
    );

    let as_exports: Vec<_> = as_module.exports().map(|e| e.name()).collect();
    let cs_exports: Vec<_> = cs_module.exports().map(|e| e.name()).collect();
    println!("AssemblyScript Exports:      {:?}", as_exports);
    println!("C# Exports:                  {:?}", cs_exports);

    // 4. Memory Footprint Estimation for 10,000 Game Rooms
    println!("\n--- [ 4. Multi-Tenant Server Density (10,000 Rooms) ] ---");
    let as_ram_mb = (10_000.0 * 64.0) / 1024.0; // 64 KB per AS room
    let cs_ram_mb = 10_000.0 * 50.0; // 50 MB per .NET Mono instance
    println!(
        "10,000 AssemblyScript Rooms: ~{:.1} MB ({:.2} GB) RAM",
        as_ram_mb,
        as_ram_mb / 1024.0
    );
    println!(
        "10,000 C# (.NET Mono) Rooms: ~{:.0} MB ({:.1} GB) RAM",
        cs_ram_mb,
        cs_ram_mb / 1024.0
    );

    println!("\n=======================================================\n");

    assert!(
        as_bytes.len() < 1000,
        "AssemblyScript stub must remain under 1KB"
    );
    assert!(cs_bytes.len() > 1_000_000, "C# bundle is in megabytes");
}
