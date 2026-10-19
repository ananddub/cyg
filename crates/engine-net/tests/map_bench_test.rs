use std::sync::Arc;
use std::time::Instant;
use ahash::RandomState;
use dashmap::DashMap;
use scc::HashMap as SccMap;

const NUM_THREADS: usize = 8;
const OPS_PER_THREAD: usize = 100_000;
const INITIAL_KEYS: usize = 20_000;

#[test]
fn test_concurrent_map_performance_and_fastest_comparison() {
    println!("\n==========================================================================");
    println!("     CONCURRENT HASHMAP TEST: DashMap vs scc::HashMap (Default vs AHash)");
    println!("     Threads: {} | Initial Keys: {} | Ops/Thread: {}", NUM_THREADS, INITIAL_KEYS, OPS_PER_THREAD);
    println!("==========================================================================");

    // 1. DashMap Default
    let (d_def_r_mops, d_def_mix_mops, d_def_w_mops) = test_dashmap_default();
    
    // 2. DashMap AHash
    let (d_ahash_r_mops, d_ahash_mix_mops, d_ahash_w_mops) = test_dashmap_ahash();

    // 3. scc::HashMap Default
    let (scc_def_r_mops, scc_def_mix_mops, scc_def_w_mops) = test_scc_default();

    // 4. scc::HashMap AHash
    let (scc_ahash_r_mops, scc_ahash_mix_mops, scc_ahash_w_mops) = test_scc_ahash();

    println!("\n==========================================================================");
    println!("                           FINAL COMPARISON SUMMARY");
    println!("==========================================================================");
    println!("{:<32} | {:<14} | {:<14} | {:<14}", "Implementation", "100% Read", "90% R / 10% W", "100% Insert");
    println!("--------------------------------------------------------------------------");
    println!("{:<32} | {:>10.2} M/s | {:>10.2} M/s | {:>10.2} M/s", "DashMap (Default)", d_def_r_mops, d_def_mix_mops, d_def_w_mops);
    println!("{:<32} | {:>10.2} M/s | {:>10.2} M/s | {:>10.2} M/s", "DashMap (AHash)", d_ahash_r_mops, d_ahash_mix_mops, d_ahash_w_mops);
    println!("{:<32} | {:>10.2} M/s | {:>10.2} M/s | {:>10.2} M/s", "scc::HashMap (Default)", scc_def_r_mops, scc_def_mix_mops, scc_def_w_mops);
    println!("{:<32} | {:>10.2} M/s | {:>10.2} M/s | {:>10.2} M/s", "scc::HashMap (AHash)", scc_ahash_r_mops, scc_ahash_mix_mops, scc_ahash_w_mops);
    println!("==========================================================================\n");

    let highest_mixed = [
        ("DashMap (Default)", d_def_mix_mops),
        ("DashMap (AHash)", d_ahash_mix_mops),
        ("scc::HashMap (Default)", scc_def_mix_mops),
        ("scc::HashMap (AHash)", scc_ahash_mix_mops),
    ];
    let mut winner = highest_mixed[0];
    for candidate in &highest_mixed[1..] {
        if candidate.1 > winner.1 {
            winner = *candidate;
        }
    }
    println!("🏆 WINNER for Real-World Workload (90% Read / 10% Write): {} ({:.2} M ops/sec)\n", winner.0, winner.1);
}

fn test_dashmap_default() -> (f64, f64, f64) {
    let map = Arc::new(DashMap::<u64, u64>::new());
    for i in 0..INITIAL_KEYS as u64 {
        map.insert(i, i * 2);
    }
    let r = run_bench({
        let map = map.clone();
        move |tid| {
            let start = tid as u64;
            let mut sum = 0u64;
            for i in 0..OPS_PER_THREAD as u64 {
                let key = (start + i) % INITIAL_KEYS as u64;
                if let Some(v) = map.get(&key) {
                    sum = sum.wrapping_add(*v);
                }
            }
            std::hint::black_box(sum);
        }
    });

    let mix = run_bench({
        let map = map.clone();
        move |tid| {
            let start = (tid * OPS_PER_THREAD) as u64;
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

    let map_w = Arc::new(DashMap::<u64, u64>::new());
    let w = run_bench({
        let map = map_w.clone();
        move |tid| {
            let base = (tid * OPS_PER_THREAD) as u64;
            for i in 0..OPS_PER_THREAD as u64 {
                map.insert(base + i, i);
            }
        }
    });
    (r, mix, w)
}

fn test_dashmap_ahash() -> (f64, f64, f64) {
    let map = Arc::new(DashMap::<u64, u64, RandomState>::with_hasher(RandomState::new()));
    for i in 0..INITIAL_KEYS as u64 {
        map.insert(i, i * 2);
    }
    let r = run_bench({
        let map = map.clone();
        move |tid| {
            let start = tid as u64;
            let mut sum = 0u64;
            for i in 0..OPS_PER_THREAD as u64 {
                let key = (start + i) % INITIAL_KEYS as u64;
                if let Some(v) = map.get(&key) {
                    sum = sum.wrapping_add(*v);
                }
            }
            std::hint::black_box(sum);
        }
    });

    let mix = run_bench({
        let map = map.clone();
        move |tid| {
            let start = (tid * OPS_PER_THREAD) as u64;
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

    let map_w = Arc::new(DashMap::<u64, u64, RandomState>::with_hasher(RandomState::new()));
    let w = run_bench({
        let map = map_w.clone();
        move |tid| {
            let base = (tid * OPS_PER_THREAD) as u64;
            for i in 0..OPS_PER_THREAD as u64 {
                map.insert(base + i, i);
            }
        }
    });
    (r, mix, w)
}

fn test_scc_default() -> (f64, f64, f64) {
    let map = Arc::new(SccMap::<u64, u64>::default());
    for i in 0..INITIAL_KEYS as u64 {
        let _ = map.insert_sync(i, i * 2);
    }
    let r = run_bench({
        let map = map.clone();
        move |tid| {
            let start = tid as u64;
            let mut sum = 0u64;
            for i in 0..OPS_PER_THREAD as u64 {
                let key = (start + i) % INITIAL_KEYS as u64;
                if let Some(v) = map.read_sync(&key, |_, val| *val) {
                    sum = sum.wrapping_add(v);
                }
            }
            std::hint::black_box(sum);
        }
    });

    let mix = run_bench({
        let map = map.clone();
        move |tid| {
            let start = (tid * OPS_PER_THREAD) as u64;
            for i in 0..OPS_PER_THREAD as u64 {
                let key = (start + i) % (INITIAL_KEYS as u64 * 2);
                if i % 10 == 0 {
                    let _ = map.upsert_sync(key, i);
                } else {
                    let _ = map.read_sync(&key, |_, val| *val);
                }
            }
        }
    });

    let map_w = Arc::new(SccMap::<u64, u64>::default());
    let w = run_bench({
        let map = map_w.clone();
        move |tid| {
            let base = (tid * OPS_PER_THREAD) as u64;
            for i in 0..OPS_PER_THREAD as u64 {
                let _ = map.insert_sync(base + i, i);
            }
        }
    });
    (r, mix, w)
}

fn test_scc_ahash() -> (f64, f64, f64) {
    let map = Arc::new(SccMap::<u64, u64, RandomState>::with_hasher(RandomState::new()));
    for i in 0..INITIAL_KEYS as u64 {
        let _ = map.insert_sync(i, i * 2);
    }
    let r = run_bench({
        let map = map.clone();
        move |tid| {
            let start = tid as u64;
            let mut sum = 0u64;
            for i in 0..OPS_PER_THREAD as u64 {
                let key = (start + i) % INITIAL_KEYS as u64;
                if let Some(v) = map.read_sync(&key, |_, val| *val) {
                    sum = sum.wrapping_add(v);
                }
            }
            std::hint::black_box(sum);
        }
    });

    let mix = run_bench({
        let map = map.clone();
        move |tid| {
            let start = (tid * OPS_PER_THREAD) as u64;
            for i in 0..OPS_PER_THREAD as u64 {
                let key = (start + i) % (INITIAL_KEYS as u64 * 2);
                if i % 10 == 0 {
                    let _ = map.upsert_sync(key, i);
                } else {
                    let _ = map.read_sync(&key, |_, val| *val);
                }
            }
        }
    });

    let map_w = Arc::new(SccMap::<u64, u64, RandomState>::with_hasher(RandomState::new()));
    let w = run_bench({
        let map = map_w.clone();
        move |tid| {
            let base = (tid * OPS_PER_THREAD) as u64;
            for i in 0..OPS_PER_THREAD as u64 {
                let _ = map.insert_sync(base + i, i);
            }
        }
    });
    (r, mix, w)
}

fn run_bench<F>(f: F) -> f64
where
    F: Fn(usize) + Send + Sync + 'static + Clone,
{
    let total_ops = NUM_THREADS * OPS_PER_THREAD;
    let mut handles = Vec::with_capacity(NUM_THREADS);
    let start = Instant::now();
    for tid in 0..NUM_THREADS {
        let task = f.clone();
        handles.push(std::thread::spawn(move || task(tid)));
    }
    for h in handles {
        h.join().unwrap();
    }
    let elapsed = start.elapsed().as_secs_f64();
    (total_ops as f64 / 1_000_000.0) / elapsed
}
