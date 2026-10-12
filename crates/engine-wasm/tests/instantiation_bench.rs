use engine_wasm::{WasmConfig, WasmRuntime};
use std::time::Instant;

const ROOMS: u32 = 10_000;

const MODULE: &str = r#"
(module
    (memory (export "memory") 1)
    (func (export "calculate_damage") (param $base i32) (param $mult f32) (result i32)
        (i32.trunc_f32_s
            (f32.mul (f32.convert_i32_s (local.get $base)) (local.get $mult))
        )
    )
)
"#;

fn bench_instantiation(name: &str, config: WasmConfig) {
    let runtime = WasmRuntime::new_with_config(config).expect("Runtime should initialize");
    let bytes = wat::parse_str(MODULE).expect("WAT parse ok");
    let module = runtime.compile(&bytes).expect("Compile ok");

    let pre = runtime
        .prepare_standalone::<()>(&module)
        .expect("Prepare ok");

    // Warmup: touch the hot paths once so first-call costs don't skew results.
    {
        let mut warm = pre.instantiate(()).expect("Warmup instantiate ok");
        let _: i32 = warm.call("calculate_damage", (1i32, 1.0f32)).expect("ok");
    }

    let start = Instant::now();
    for _ in 0..ROOMS {
        let mut room = pre.instantiate(()).expect("Instantiate ok");
        std::hint::black_box(&mut room);
    }
    let elapsed = start.elapsed();

    let per_room_ns = elapsed.as_nanos() / u128::from(ROOMS);
    let per_sec = (ROOMS as f64) / elapsed.as_secs_f64();

    println!();
    println!("--- [{name}] Instantiation Results ---");
    println!("Rooms spawned:          {ROOMS}");
    println!("Total time:             {:.4?}", elapsed);
    println!("Avg latency per room:   {} ns", per_room_ns);
    println!("Throughput:             ~{:.0} rooms/sec", per_sec);
    println!("=======================================================");
}

#[test]
fn benchmark_ondemand_vs_pooling_instantiation() {
    println!();
    println!("=======================================================");
    println!(" ON-DEMAND vs POOLING ALLOCATOR INSTANTIATION BENCH");
    println!(" ({ROOMS} rooms each, pre-instantiated template)");
    println!("=======================================================");

    bench_instantiation("On-Demand Allocator", WasmConfig::default());
    bench_instantiation(
        "Pooling Allocator",
        WasmConfig {
            use_pooling_allocator: true,
            ..WasmConfig::default()
        },
    );
}
