use std::time::Instant;
use engine_wasm::{WasmRuntime, wasm_module};

// 1. Define a rich, multi-signature game interface
wasm_module! {
    pub struct BattleRoom {
        fn reset();
        fn set_speed(speed: f32);
        fn spawn_entity(entity_id: u32, x: f32, y: f32, team: i32) -> u32;
        fn calculate_exp(base: i32, multiplier: f32) -> i32;
        fn cause_trap();
    }
}

// Custom Host State to test linker integration
#[derive(Default)]
struct ServerSessionState {
    pub events_count: u32,
}

#[test]
fn test_macro_comprehensive_suite() {
    println!("\n=======================================================");
    println!("     WASM_MODULE! MACRO COMPREHENSIVE TEST SUITE       ");
    println!("=======================================================");

    let runtime = WasmRuntime::new().expect("Runtime initialization");

    let wat = r#"
    (module
        (import "env" "notify_host" (func $notify_host))
        (memory (export "memory") 1)

        (global $speed (mut f32) (f32.const 100.0))
        (global $entity_count (mut i32) (i32.const 0))

        ;; 0 args
        (func (export "reset")
            (global.set $speed (f32.const 0.0))
            (global.set $entity_count (i32.const 0))
            (call $notify_host)
        )

        ;; 1 arg
        (func (export "set_speed") (param $s f32)
            (global.set $speed (local.get $s))
        )

        ;; 4 args with return value
        (func (export "spawn_entity") (param $id i32) (param $x f32) (param $y f32) (param $team i32) (result i32)
            (global.set $entity_count (i32.add (global.get $entity_count) (i32.const 1)))
            ;; Return compound hash id
            (i32.add (local.get $id) (local.get $team))
        )

        ;; 2 args math calculation
        (func (export "calculate_exp") (param $b i32) (param $m f32) (result i32)
            (i32.trunc_f32_s
                (f32.mul (f32.convert_i32_s (local.get $b)) (local.get $m))
            )
        )

        ;; Intentional trap (unreachable instruction)
        (func (export "cause_trap")
            (unreachable)
        )
    )
    "#;

    let wasm_bytes = wat::parse_str(wat).expect("WAT parse ok");
    let module = runtime.compile(&wasm_bytes).expect("Compile ok");

    // Test Host Linker Integration
    let mut linker = runtime.create_linker::<ServerSessionState>();
    linker
        .func_wrap("env", "notify_host", |mut caller: wasmtime::Caller<'_, ServerSessionState>| {
            caller.data_mut().events_count += 1;
        })
        .expect("Linker wrap ok");

    let initial_state = ServerSessionState { events_count: 0 };
    let mut room = BattleRoom::from_module_with_linker(&runtime, &linker, &module, initial_state)
        .expect("Instantiation ok");

    // --- TEST 1: 0-arg call and Host State Mutation ---
    room.reset().expect("reset call ok");
    assert_eq!(room.data().events_count, 1, "Host state should be updated via host import");
    println!("✓ Test 1: 0-arg call & Host Linker integration passed");

    // --- TEST 2: 1-arg call ---
    room.set_speed(250.5).expect("set_speed ok");
    println!("✓ Test 2: 1-arg call passed");

    // --- TEST 3: 4-args call with return value ---
    let spawned_id = room.spawn_entity(1001, 12.5, 45.0, 2).expect("spawn_entity ok");
    assert_eq!(spawned_id, 1003);
    println!("✓ Test 3: Multi-args (4 parameters) call passed");

    // --- TEST 4: Direct Linear Memory Read/Write via Macro Struct ---
    let payload = b"Hello from Macro Memory API";
    room.write_memory(64, payload).expect("write_memory ok");
    let read_back = room.read_memory(64, payload.len() as u32).expect("read_memory ok");
    assert_eq!(read_back, payload);
    println!("✓ Test 4: Macro memory read/write passed");

    // --- TEST 5: Graceful Trap / Error Handling ---
    let trap_result = room.cause_trap();
    assert!(trap_result.is_err(), "cause_trap must return an Err");
    println!("✓ Test 5: Graceful trap error handling passed: {:?}", trap_result.err().unwrap());

    // --- TEST 6: Macro Zero-Overhead Benchmark (1,000,000 calls) ---
    let iterations = 1_000_000;
    let start_bench = Instant::now();

    for i in 0..iterations {
        let _ = room.calculate_exp(i as i32, 1.2).unwrap();
    }

    let elapsed = start_bench.elapsed();
    let ns_per_call = (elapsed.as_nanos() as f64) / (iterations as f64);

    println!("\n--- [Macro Call Overhead Benchmark] ---");
    println!("Total Time for 1,000,000 Macro Calls: {:?}", elapsed);
    println!("Average Time per Macro Call:          {:.2} nanoseconds", ns_per_call);
    println!("Throughput:                           {:.0} calls/sec", (iterations as f64) / elapsed.as_secs_f64());
    println!("=======================================================\n");

    assert!(ns_per_call < 200.0, "Macro calling overhead must be under 200ns");
}
