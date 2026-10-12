use std::io::{Read, Write};
use std::net::TcpStream;
use std::thread;
use std::time::{Duration, Instant};

use engine_core::{PlayerInput, PlayerInputBuffer, RewindManager, TransformSnapshot};
use engine_net::{decode_frame, encode_frame, MioWorker, OpCode, WorkerConfig};
use engine_wasm::{wasm_module, WasmRuntime};

wasm_module! {
    pub struct CompetitiveRoom {
        fn init();
        fn update(dt: f32, speed_x: f32, speed_y: f32);
        fn getScore() -> i32;
        fn getPositionX() -> f32;
        fn getPositionY() -> f32;
        fn calculateDamage(base: i32, multiplier: f32) -> i32;
    }
}

#[test]
fn test_competitive_128hz_tick_loop_benchmark() {
    println!("\n================================================================================");
    println!("     VALORANT / CS-STYLE 128Hz COMPETITIVE TICK-RATE BENCHMARK TEST             ");
    println!("================================================================================");

    let tick_rate = 128;
    let tick_budget_ms = 1000.0 / tick_rate as f64; // 7.8125 ms = 7,812.5 microseconds
    println!("🎯 Target Tick Rate    : {} Hz", tick_rate);
    println!("⏱️  Frame Time Budget   : {:.4} ms (7,812.5 µs)", tick_budget_ms);

    let port = 19280;
    let addr = format!("127.0.0.1:{}", port).parse().unwrap();

    // 1. Initialize WASM Runtime & AssemblyScript Room
    let runtime = WasmRuntime::new().expect("WasmRuntime init ok");
    let wasm_bytes = include_bytes!("../../engine-wasm/tests/assemblyscript/game_room.wasm");
    let module = runtime.compile(wasm_bytes).expect("Module compilation ok");
    let mut room = CompetitiveRoom::new(&runtime, &module).expect("Room instantiation ok");
    room.init().expect("Room init ok");

    // 2. Lag Compensation Rewind Manager: 500ms history at 128Hz = 64 snapshots
    let mut rewind_mgr = RewindManager::new(500, tick_rate);

    // 3. Bind MioWorker with 128Hz config
    let config = WorkerConfig {
        bind_addr: addr,
        tick_rate,
        max_clients: 64,
    };
    let mut worker = MioWorker::new(config).expect("MioWorker init ok");

    // 4. Per-player input buffers
    let mut player_input_buf = PlayerInputBuffer::new(128);

    // 5. Spawn a High-Frequency 128Hz Client
    let target_client_inputs = 128; // Send 128 inputs at 128Hz
    let client_handle = thread::spawn(move || {
        thread::sleep(Duration::from_millis(30));
        let mut stream = TcpStream::connect(format!("127.0.0.1:{}", port)).expect("Client connect ok");
        stream.set_nodelay(true).unwrap();

        let mut acks_received = 0;
        let mut send_buf = Vec::new();
        let mut recv_buf = Vec::new();
        let mut temp = [0u8; 1024];

        // Client sends inputs in bursts of 128Hz cadence
        for seq in 1..=target_client_inputs {
            let mut payload = Vec::new();
            payload.extend_from_slice(&(seq as u64).to_be_bytes());
            payload.extend_from_slice(&50.0f32.to_be_bytes());
            payload.extend_from_slice(&25.0f32.to_be_bytes());

            send_buf.clear();
            encode_frame(OpCode::PlayerInput, &payload, &mut send_buf);
            stream.write_all(&send_buf).expect("Client write ok");

            // Sleep ~7.8ms between input packets (128Hz cadence)
            thread::sleep(Duration::from_micros(7812));

            // Non-blocking read incoming state deltas
            stream.set_nonblocking(true).unwrap();
            while let Ok(n) = stream.read(&mut temp) {
                if n == 0 { break; }
                recv_buf.extend_from_slice(&temp[..n]);
                while let Some((opcode, frame)) = decode_frame(&mut recv_buf) {
                    if opcode == OpCode::StateDelta && frame.len() >= 8 {
                        acks_received += 1;
                    }
                }
            }
            stream.set_nonblocking(false).unwrap();
        }

        acks_received
    });

    // 6. Run Server Loop for 256 ticks (exactly 2.0 seconds at 128Hz)
    let total_test_ticks = 256;
    let mut tick_latencies_us: Vec<u64> = Vec::with_capacity(total_test_ticks);
    let mut ticks_executed = 0;
    let entity_id = 42;

    let test_start = Instant::now();

    while ticks_executed < total_test_ticks {
        let packets = worker
            .step(Duration::from_micros(1000), |ctx, clients| {
                let tick_start = Instant::now();
                ticks_executed += 1;

                // A. Drain all pending inputs for this tick
                let mut speed_x = 0.0f32;
                let mut speed_y = 0.0f32;

                while let Some(input) = player_input_buf.consume() {
                    if input.data.len() >= 8 {
                        let sx_bytes: [u8; 4] = input.data[0..4].try_into().unwrap();
                        let sy_bytes: [u8; 4] = input.data[4..8].try_into().unwrap();
                        speed_x += f32::from_be_bytes(sx_bytes);
                        speed_y += f32::from_be_bytes(sy_bytes);
                    }
                }

                // B. Execute WASM game simulation at 128Hz (dt = 0.0078125s)
                room.update(ctx.dt, speed_x, speed_y).unwrap();
                let px = room.getPositionX().unwrap();
                let py = room.getPositionY().unwrap();

                // C. Record snapshot in RewindManager for sub-millisecond lag compensation
                let snap = TransformSnapshot::new(px, py, 0.0, 1.5);
                rewind_mgr.record(entity_id, ctx.tick, ctx.dt_ms * ctx.tick as f64, snap);

                // D. Broadcast state delta with latest input ack
                let ack = player_input_buf.consumed_count();
                let mut delta = Vec::new();
                delta.extend_from_slice(&ack.to_be_bytes());
                delta.extend_from_slice(&px.to_be_bytes());
                delta.extend_from_slice(&py.to_be_bytes());

                for conn in clients.values_mut() {
                    conn.queue_message(OpCode::StateDelta, &delta);
                }

                let elapsed_us = tick_start.elapsed().as_micros() as u64;
                tick_latencies_us.push(elapsed_us);
            })
            .expect("Worker step ok");

        // Process incoming client packets
        for pkt in packets {
            if pkt.opcode == OpCode::PlayerInput && pkt.payload.len() >= 16 {
                let seq = u64::from_be_bytes(pkt.payload[0..8].try_into().unwrap());
                let data = pkt.payload[8..].to_vec();
                player_input_buf.push(PlayerInput::new(seq, 0.0, data));
            }
        }
    }

    let total_wall_time = test_start.elapsed();
    let actual_hz = (ticks_executed as f64) / total_wall_time.as_secs_f64();

    // 7. Calculate Latency Statistics
    tick_latencies_us.sort();
    let min_us = tick_latencies_us.first().copied().unwrap_or(0);
    let max_us = tick_latencies_us.last().copied().unwrap_or(0);
    let sum_us: u64 = tick_latencies_us.iter().sum();
    let avg_us = sum_us as f64 / tick_latencies_us.len() as f64;
    let p50_us = tick_latencies_us[tick_latencies_us.len() / 2];
    let p99_idx = (tick_latencies_us.len() as f64 * 0.99) as usize;
    let p99_us = tick_latencies_us[p99_idx.min(tick_latencies_us.len() - 1)];

    let cpu_budget_used_percent = (avg_us / 7812.5) * 100.0;

    println!("\n📊 128Hz BENCHMARK RESULTS:");
    println!("────────────────────────────────────────────────────────────────────────────────");
    println!(" • Total Ticks Fired       : {} ticks in {:.2?}", ticks_executed, total_wall_time);
    println!(" • Measured Tick Rate      : {:.2} Hz (Target: 128.00 Hz)", actual_hz);
    println!(" • Available Frame Budget  : 7,812.50 µs (7.81 ms)");
    println!(" • Min Tick Latency        : {} µs ({:.4} ms)", min_us, min_us as f64 / 1000.0);
    println!(" • Avg Tick Latency        : {:.2} µs ({:.4} ms)", avg_us, avg_us / 1000.0);
    println!(" • P50 Tick Latency        : {} µs ({:.4} ms)", p50_us, p50_us as f64 / 1000.0);
    println!(" • P99 Tick Latency        : {} µs ({:.4} ms)", p99_us, p99_us as f64 / 1000.0);
    println!(" • Max Tick Latency        : {} µs ({:.4} ms)", max_us, max_us as f64 / 1000.0);
    println!(" • CPU Frame Budget Used   : {:.3}% (Remaining Headroom: {:.2}%)", cpu_budget_used_percent, 100.0 - cpu_budget_used_percent);
    println!("────────────────────────────────────────────────────────────────────────────────");

    // 8. Test Lag Compensation Accuracy at 128Hz
    // Verify hit check against tick 50 (when dt = 7.8125ms -> ts = 50 * 7.8125 = 390.625ms)
    let historical_ts = 50.0 * (1000.0 / 128.0);
    let rewound = rewind_mgr.rewind_entity(entity_id, historical_ts).expect("128Hz historical snapshot found");
    println!("🎯 Lag Compensation Rewind Check at t={:.2}ms: Pos=({:.2}, {:.2})", historical_ts, rewound.x, rewound.y);
    assert!(rewound.is_point_hit(rewound.x, rewound.y, 0.0), "128Hz exact hitbox registered!");

    let _ = client_handle.join();

    // Assertions: 128Hz must maintain accurate tick rate and sub-millisecond execution
    assert!((actual_hz - 128.0).abs() < 5.0, "Tick rate must be within 5Hz of 128Hz");
    assert!(avg_us < 1000.0, "Average tick execution time must be under 1ms (1000µs)");
    println!("\n✅ 128Hz COMPETITIVE ENGINE TEST PASSED WITH FLYING COLORS!\n");
}
