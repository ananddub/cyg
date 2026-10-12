//! Honest, anti-lie benchmark: addresses the classic micro-benchmark pitfalls.
//!
//! - `black_box` on every consumed result (no dead-code elimination)
//! - Realistic host state (players Vec + HashMap + timers) instead of `()`
//! - Tail latency: p50 / p99 / p99.9, not just the mean
//! - Multi-threaded contention: shared pre-instantiated template across threads
//!
//! Run: cargo test --release -p engine-wasm --test realistic_bench -- --nocapture

use engine_wasm::{WasmConfig, WasmRuntime};
use std::collections::HashMap;
use std::hint::black_box;
use std::time::Instant;

const ROOMS_PER_THREAD: u32 = 10_000;

/// A realistic per-room host state, like a real game room would carry.
#[derive(Default)]
struct RoomState {
    players: HashMap<u64, (f32, f32, f32)>,
    packets: Vec<Vec<u8>>,
}

const MODULE: &str = r#"
(module
    (memory (export "memory") 1)
    (func (export "on_tick") (param $dt f32)
        (nop)
    )
)
"#;

fn percentile(sorted: &[u32], p: f64) -> u32 {
    let idx = (((sorted.len() - 1) as f64) * p).round() as usize;
    sorted[idx]
}

fn print_distribution(mut samples: Vec<u32>, label: &str) {
    samples.sort_unstable();
    let mean: u64 = samples.iter().map(|&s| s as u64).sum::<u64>() / samples.len() as u64;
    println!(
        "  {:<28} mean {:>7} ns | p50 {:>7} ns | p99 {:>7} ns | p99.9 {:>7} ns | max {:>7} ns",
        label,
        mean,
        percentile(&samples, 0.50),
        percentile(&samples, 0.99),
        percentile(&samples, 0.999),
        samples[samples.len() - 1],
    );
}

fn bench_spawn(pre: &engine_wasm::WasmPreInstance<RoomState>) -> Vec<u32> {
    let mut samples = Vec::with_capacity(ROOMS_PER_THREAD as usize);
    for _ in 0..ROOMS_PER_THREAD {
        let start = Instant::now();
        let mut room = pre
            .instantiate(RoomState {
                players: HashMap::with_capacity(64),
                packets: Vec::with_capacity(256),
            })
            .expect("instantiate ok");
        // Realistic touch: run a tick and write a packet so nothing is optimized away.
        let on_tick: wasmtime::TypedFunc<(f32,), ()> =
            room.get_typed_func("on_tick").expect("export ok");
        on_tick
            .call(room.store_mut(), (0.016f32,))
            .expect("call ok");
        room.write_memory(0, &[0u8; 64]).expect("write ok");
        black_box(room.data().players.capacity());
        black_box(room.data_mut().packets.capacity());
        black_box(room);
        samples.push(start.elapsed().as_nanos() as u32);
    }
    samples
}

#[test]
fn honest_benchmark_single_threaded() {
    let runtime = WasmRuntime::new().expect("runtime");
    let bytes = wat::parse_str(MODULE).expect("wat ok");
    let module = runtime.compile(&bytes).expect("compile ok");

    println!();
    println!(
        "=== HONEST BENCH: realistic state, p99, single thread ({ROOMS_PER_THREAD} rooms) ==="
    );

    let pre_ondemand = runtime
        .prepare_standalone::<RoomState>(&module)
        .expect("pre ok");
    let samples = bench_spawn(&pre_ondemand);
    print_distribution(samples, "On-Demand + InstancePre");

    let runtime_pool = WasmRuntime::new_with_config(WasmConfig {
        use_pooling_allocator: true,
        ..WasmConfig::default()
    })
    .expect("runtime");
    let module_pool = {
        let bytes = wat::parse_str(MODULE).expect("wat ok");
        runtime_pool.compile(&bytes).expect("compile ok")
    };
    let pre_pool = runtime_pool
        .prepare_standalone::<RoomState>(&module_pool)
        .expect("pre ok");
    let samples = bench_spawn(&pre_pool);
    print_distribution(samples, "Pooling + InstancePre");
}

#[test]
fn honest_benchmark_multithreaded_contention() {
    let runtime = WasmRuntime::new_with_config(WasmConfig {
        use_pooling_allocator: true,
        ..WasmConfig::default()
    })
    .expect("runtime");
    let bytes = wat::parse_str(MODULE).expect("wat ok");
    let module = runtime.compile(&bytes).expect("compile ok");
    let pre = runtime
        .prepare_standalone::<RoomState>(&module)
        .expect("pre ok");

    let threads = 4;
    println!();
    println!(
        "=== HONEST BENCH: {threads} threads sharing ONE template ({ROOMS_PER_THREAD} rooms each) ==="
    );

    let start = Instant::now();
    std::thread::scope(|s| {
        let mut handles = Vec::new();
        for t in 0..threads {
            let pre = &pre;
            handles.push(s.spawn(move || {
                let samples = bench_spawn(pre);
                (t, samples)
            }));
        }
        for h in handles {
            let (t, samples) = h.join().expect("thread ok");
            print_distribution(samples, &format!("thread-{t} (contended)"));
        }
    });
    let total = start.elapsed();
    let rooms = threads * ROOMS_PER_THREAD;
    println!(
        "  Total: {} rooms in {:.4?} -> ~{:.0} rooms/sec aggregate",
        rooms,
        total,
        rooms as f64 / total.as_secs_f64()
    );
}
