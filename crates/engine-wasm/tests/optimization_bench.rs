use std::time::Instant;
use wasmtime::{Config, Engine, Instance, Module, OptLevel, Store, Strategy, TypedFunc};

#[test]
fn benchmark_cached_typed_func_vs_string_lookup() {
    println!("\n=======================================================");
    println!("     WASM PERFORMANCE OPTIMIZATION BENCHMARK           ");
    println!("     (Cached TypedFunc vs Per-Call String Lookup)      ");
    println!("=======================================================");

    let mut config = Config::new();
    config.strategy(Strategy::Cranelift);
    config.cranelift_opt_level(OptLevel::Speed);

    let engine = Engine::new(&config).unwrap();

    let wat = r#"
    (module
        (func (export "calculate_exp") (param $b i32) (param $m f32) (result i32)
            (i32.trunc_f32_s
                (f32.mul (f32.convert_i32_s (local.get $b)) (local.get $m))
            )
        )
    )
    "#;

    let wasm_bytes = wat::parse_str(wat).unwrap();
    let module = Module::new(&engine, &wasm_bytes).unwrap();
    let mut store = Store::new(&engine, ());
    let instance = Instance::new(&mut store, &module, &[]).unwrap();

    let iterations = 1_000_000;

    // APPROACH 1: String Lookup on Every Call (What our un-cached macro did: ~128ns)
    let start_uncached = Instant::now();
    for i in 0..iterations {
        let func = instance
            .get_typed_func::<(i32, f32), i32>(&mut store, "calculate_exp")
            .unwrap();
        let _ = func.call(&mut store, (i as i32, 1.2)).unwrap();
    }
    let dur_uncached = start_uncached.elapsed();
    let ns_uncached = (dur_uncached.as_nanos() as f64) / (iterations as f64);

    // APPROACH 2: Cached TypedFunc (What the optimized engine does: ~17ns)
    let cached_func: TypedFunc<(i32, f32), i32> = instance
        .get_typed_func::<(i32, f32), i32>(&mut store, "calculate_exp")
        .unwrap();

    let start_cached = Instant::now();
    for i in 0..iterations {
        let _ = cached_func.call(&mut store, (i as i32, 1.2)).unwrap();
    }
    let dur_cached = start_cached.elapsed();
    let ns_cached = (dur_cached.as_nanos() as f64) / (iterations as f64);

    println!(
        "1. Uncached (Per-call String Lookup):  {:.2} ns  ({:.0} calls/sec)",
        ns_uncached,
        (iterations as f64) / dur_uncached.as_secs_f64()
    );
    println!(
        "2. Cached TypedFunc (Direct Pointer):  {:.2} ns  ({:.0} calls/sec)",
        ns_cached,
        (iterations as f64) / dur_cached.as_secs_f64()
    );
    println!("-------------------------------------------------------");
    println!(
        "SPEEDUP: Cached TypedFunc is {:.1}x FASTER!",
        ns_uncached / ns_cached
    );
    println!("=======================================================\n");
}
