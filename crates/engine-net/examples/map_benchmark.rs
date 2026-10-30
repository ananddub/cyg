use ahash::RandomState;
use dashmap::DashMap;
use scc::HashMap as SccMap;
use std::sync::Arc;
use std::time::Instant;

const NUM_THREADS: usize = 16;
const OPS_PER_THREAD: usize = 200_000;
const INITIAL_KEYS: usize = 50_000;

fn main() {
    println!("==========================================================================");
    println!("     CONCURRENT HASHMAP BENCHMARK: DashMap vs scc::HashMap (Default vs AHash)");
    println!(
        "     CPU Threads: {} | Initial Keys: {} | Ops/Thread: {}",
        NUM_THREADS, INITIAL_KEYS, OPS_PER_THREAD
    );
    println!("==========================================================================\n");

    // 1. DashMap with Default Hasher
    println!(">>> Benchmarking 1: DashMap (Default Hasher) <<<");
    bench_dashmap_default();

    // 2. DashMap with AHash
    println!("\n>>> Benchmarking 2: DashMap (AHash) <<<");
    bench_dashmap_ahash();

    // 3. scc::HashMap with Default Hasher
    println!("\n>>> Benchmarking 3: scc::HashMap (Default Hasher) <<<");
    bench_scc_default();

    // 4. scc::HashMap with AHash
    println!("\n>>> Benchmarking 4: scc::HashMap (AHash) <<<");
    bench_scc_ahash();

    println!("\n==========================================================================");
    println!("                         BENCHMARK COMPLETE");
    println!("==========================================================================");
}

// -----------------------------------------------------------------------------
// 1. DashMap (Default)
// -----------------------------------------------------------------------------
fn bench_dashmap_default() {
    let map = Arc::new(DashMap::<u64, u64>::new());
    for i in 0..INITIAL_KEYS as u64 {
        map.insert(i, i * 2);
    }

    // Benchmark A: 100% Reads
    run_benchmark("DashMap (Default) - 100% Reads", {
        let map = map.clone();
        move |thread_id| {
            let start = thread_id as u64;
            let mut sum = 0u64;
            for i in 0..OPS_PER_THREAD as u64 {
                let key = (start + i) % INITIAL_KEYS as u64;
                if let Some(val) = map.get(&key) {
                    sum = sum.wrapping_add(*val);
                }
            }
            std::hint::black_box(sum);
        }
    });

    // Benchmark B: 90% Reads / 10% Writes (Realistic Game Routing/Session Cache)
    run_benchmark("DashMap (Default) - 90% Read / 10% Write", {
        let map = map.clone();
        move |thread_id| {
            let start = (thread_id * OPS_PER_THREAD) as u64;
            for i in 0..OPS_PER_THREAD as u64 {
                let key = (start + i) % (INITIAL_KEYS as u64 * 2);
                if i % 10 == 0 {
                    map.insert(key, i);
                } else {
                    let _ = map.get(&key);
                }
            }
        }
    });

    // Benchmark C: 100% Concurrent Inserts
    let map_write = Arc::new(DashMap::<u64, u64>::new());
    run_benchmark("DashMap (Default) - 100% Inserts", {
        let map = map_write.clone();
        move |thread_id| {
            let base = (thread_id * OPS_PER_THREAD) as u64;
            for i in 0..OPS_PER_THREAD as u64 {
                map.insert(base + i, i);
            }
        }
    });
}

// -----------------------------------------------------------------------------
// 2. DashMap (AHash)
// -----------------------------------------------------------------------------
fn bench_dashmap_ahash() {
    let map = Arc::new(DashMap::<u64, u64, RandomState>::with_hasher(
        RandomState::new(),
    ));
    for i in 0..INITIAL_KEYS as u64 {
        map.insert(i, i * 2);
    }

    run_benchmark("DashMap (AHash) - 100% Reads", {
        let map = map.clone();
        move |thread_id| {
            let start = thread_id as u64;
            let mut sum = 0u64;
            for i in 0..OPS_PER_THREAD as u64 {
                let key = (start + i) % INITIAL_KEYS as u64;
                if let Some(val) = map.get(&key) {
                    sum = sum.wrapping_add(*val);
                }
            }
            std::hint::black_box(sum);
        }
    });

    run_benchmark("DashMap (AHash) - 90% Read / 10% Write", {
        let map = map.clone();
        move |thread_id| {
            let start = (thread_id * OPS_PER_THREAD) as u64;
            for i in 0..OPS_PER_THREAD as u64 {
                let key = (start + i) % (INITIAL_KEYS as u64 * 2);
                if i % 10 == 0 {
                    map.insert(key, i);
                } else {
                    let _ = map.get(&key);
                }
            }
        }
    });

    let map_write = Arc::new(DashMap::<u64, u64, RandomState>::with_hasher(
        RandomState::new(),
    ));
    run_benchmark("DashMap (AHash) - 100% Inserts", {
        let map = map_write.clone();
        move |thread_id| {
            let base = (thread_id * OPS_PER_THREAD) as u64;
            for i in 0..OPS_PER_THREAD as u64 {
                map.insert(base + i, i);
            }
        }
    });
}

// -----------------------------------------------------------------------------
// 3. scc::HashMap (Default)
// -----------------------------------------------------------------------------
fn bench_scc_default() {
    let map = Arc::new(SccMap::<u64, u64>::default());
    for i in 0..INITIAL_KEYS as u64 {
        let _ = map.insert_sync(i, i * 2);
    }

    run_benchmark("scc::HashMap (Default) - 100% Reads", {
        let map = map.clone();
        move |thread_id| {
            let start = thread_id as u64;
            let mut sum = 0u64;
            for i in 0..OPS_PER_THREAD as u64 {
                let key = (start + i) % INITIAL_KEYS as u64;
                if let Some(val) = map.read_sync(&key, |_, v| *v) {
                    sum = sum.wrapping_add(val);
                }
            }
            std::hint::black_box(sum);
        }
    });

    run_benchmark("scc::HashMap (Default) - 90% Read / 10% Write", {
        let map = map.clone();
        move |thread_id| {
            let start = (thread_id * OPS_PER_THREAD) as u64;
            for i in 0..OPS_PER_THREAD as u64 {
                let key = (start + i) % (INITIAL_KEYS as u64 * 2);
                if i % 10 == 0 {
                    let _ = map.upsert_sync(key, i);
                } else {
                    let _ = map.read_sync(&key, |_, v| *v);
                }
            }
        }
    });

    let map_write = Arc::new(SccMap::<u64, u64>::default());
    run_benchmark("scc::HashMap (Default) - 100% Inserts", {
        let map = map_write.clone();
        move |thread_id| {
            let base = (thread_id * OPS_PER_THREAD) as u64;
            for i in 0..OPS_PER_THREAD as u64 {
                let _ = map.insert_sync(base + i, i);
            }
        }
    });
}

// -----------------------------------------------------------------------------
// 4. scc::HashMap (AHash)
// -----------------------------------------------------------------------------
fn bench_scc_ahash() {
    let map = Arc::new(SccMap::<u64, u64, RandomState>::with_hasher(
        RandomState::new(),
    ));
    for i in 0..INITIAL_KEYS as u64 {
        let _ = map.insert_sync(i, i * 2);
    }

    run_benchmark("scc::HashMap (AHash) - 100% Reads", {
        let map = map.clone();
        move |thread_id| {
            let start = thread_id as u64;
            let mut sum = 0u64;
            for i in 0..OPS_PER_THREAD as u64 {
                let key = (start + i) % INITIAL_KEYS as u64;
                if let Some(val) = map.read_sync(&key, |_, v| *v) {
                    sum = sum.wrapping_add(val);
                }
            }
            std::hint::black_box(sum);
        }
    });

    run_benchmark("scc::HashMap (AHash) - 90% Read / 10% Write", {
        let map = map.clone();
        move |thread_id| {
            let start = (thread_id * OPS_PER_THREAD) as u64;
            for i in 0..OPS_PER_THREAD as u64 {
                let key = (start + i) % (INITIAL_KEYS as u64 * 2);
                if i % 10 == 0 {
                    let _ = map.upsert_sync(key, i);
                } else {
                    let _ = map.read_sync(&key, |_, v| *v);
                }
            }
        }
    });

    let map_write = Arc::new(SccMap::<u64, u64, RandomState>::with_hasher(
        RandomState::new(),
    ));
    run_benchmark("scc::HashMap (AHash) - 100% Inserts", {
        let map = map_write.clone();
        move |thread_id| {
            let base = (thread_id * OPS_PER_THREAD) as u64;
            for i in 0..OPS_PER_THREAD as u64 {
                let _ = map.insert_sync(base + i, i);
            }
        }
    });
}

// -----------------------------------------------------------------------------
// Benchmark Runner Helper
// -----------------------------------------------------------------------------
fn run_benchmark<F>(name: &str, task_factory: F)
where
    F: Fn(usize) + Send + Sync + 'static + Clone,
{
    let total_ops = NUM_THREADS * OPS_PER_THREAD;
    let mut handles = Vec::with_capacity(NUM_THREADS);

    let start = Instant::now();
    for thread_id in 0..NUM_THREADS {
        let task = task_factory.clone();
        handles.push(std::thread::spawn(move || {
            task(thread_id);
        }));
    }

    for h in handles {
        h.join().unwrap();
    }
    let elapsed = start.elapsed();
    let elapsed_secs = elapsed.as_secs_f64();
    let mops = (total_ops as f64 / 1_000_000.0) / elapsed_secs;
    let ns_per_op = (elapsed.as_nanos() as f64) / (total_ops as f64);

    println!(
        "  {:<42} | Time: {:>7.2?} | {:>7.2} M ops/sec | {:>6.2} ns/op",
        name, elapsed, mops, ns_per_op
    );
}
