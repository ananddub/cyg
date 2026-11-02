use std::time::Instant;

use super::fixture::open_room;

#[test]
fn test_macro_call_overhead_benchmark() {
    let mut room = open_room();

    let iterations = 1_000_000;
    let start_bench = Instant::now();

    for i in 0..iterations {
        let _ = room.calculate_exp(i as i32, 1.2).unwrap();
    }

    let elapsed = start_bench.elapsed();
    let ns_per_call = (elapsed.as_nanos() as f64) / (iterations as f64);

    println!("\n--- [Macro Call Overhead Benchmark] ---");
    println!("Total Time for 1,000,000 Macro Calls: {:?}", elapsed);
    println!(
        "Average Time per Macro Call:          {:.2} nanoseconds",
        ns_per_call
    );
    println!(
        "Throughput:                           {:.0} calls/sec",
        (iterations as f64) / elapsed.as_secs_f64()
    );

    if cfg!(debug_assertions) {
        assert!(
            ns_per_call < 3000.0,
            "Debug mode macro call overhead must be reasonable"
        );
    } else {
        assert!(
            ns_per_call < 200.0,
            "Macro calling overhead must be under 200ns in release mode"
        );
    }
}
