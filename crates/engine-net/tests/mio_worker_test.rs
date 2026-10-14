use std::io::{Read, Write};
use std::net::TcpStream;
use std::thread;
use std::time::Duration;

use engine_net::{decode_frame, encode_frame, MioWorker, OpCode, WorkerConfig};

#[test]
fn test_mio_worker_client_echo_and_ticks() {
    println!("\n=== TEST: Raw Mio Worker Thread-Per-Core Network Test ===");

    let port = 18080;
    let addr = format!("127.0.0.1:{}", port).parse().unwrap();

    let config = WorkerConfig {
        bind_addr: addr,
        tick_rate: 60,
        max_clients: 128,
    };

    let mut worker = MioWorker::new(config).expect("Worker creation and SO_REUSEPORT bind ok");
    println!("✓ MioWorker bound to 127.0.0.1:{} with SO_REUSEPORT", port);

    // Spawn a test client thread
    let client_handle = thread::spawn(move || {
        // Give worker a moment to enter event loop
        thread::sleep(Duration::from_millis(50));

        let mut stream = TcpStream::connect(format!("127.0.0.1:{}", port)).expect("Client connected");
        stream.set_nodelay(true).unwrap();
        stream.set_read_timeout(Some(Duration::from_millis(500))).unwrap();

        // 1. Send PlayerInput packet: opcode = PlayerInput, payload = [42, 100, 200]
        let mut send_buf = Vec::new();
        encode_frame(OpCode::PlayerInput, &[42, 100, 200], &mut send_buf);
        stream.write_all(&send_buf).expect("Write ok");
        stream.flush().expect("Flush ok");

        // 2. Read response from server
        let mut recv_buf = Vec::new();
        let mut temp = [0u8; 1024];
        let deadline = std::time::Instant::now() + Duration::from_secs(2);
        while std::time::Instant::now() < deadline {
            match stream.read(&mut temp) {
                Ok(0) => break,
                Ok(n) => {
                    recv_buf.extend_from_slice(&temp[..n]);
                    if let Some((opcode, payload)) = decode_frame(&mut recv_buf) {
                        return (opcode, payload);
                    }
                }
                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock || e.kind() == std::io::ErrorKind::TimedOut => {
                    thread::sleep(Duration::from_millis(5));
                }
                Err(e) => panic!("Read error: {}", e),
            }
        }
        panic!("Timeout waiting for response from MioWorker");
    });

    // Run worker steps
    let mut ticks_fired = 0;
    let mut packet_received = false;

    let test_deadline = std::time::Instant::now() + Duration::from_secs(2);
    let mut packets = Vec::new();
    while std::time::Instant::now() < test_deadline {
        worker
            .step(Duration::from_millis(5), &mut packets, |_ctx, clients| {
                ticks_fired += 1;
                // On tick, send a StateDelta back to all clients
                for conn in clients.values_mut() {
                    conn.queue_message(OpCode::StateDelta, &[99, 88, 77]);
                }
            })
            .expect("Step ok");

        for pkt in packets.drain(..) {
            if pkt.opcode == OpCode::PlayerInput && pkt.payload == vec![42, 100, 200] {
                packet_received = true;
                println!("✓ Worker received PlayerInput frame from {}: {:?}", pkt.session_id, pkt.payload);
            }
        }

        if client_handle.is_finished() {
            break;
        }
    }

    let (client_recv_opcode, client_recv_payload) = client_handle.join().expect("Client thread joined");
    assert_eq!(client_recv_opcode, OpCode::StateDelta);
    assert_eq!(client_recv_payload, vec![99, 88, 77]);
    println!("✓ Client received StateDelta response: {:?}", client_recv_payload);

    assert!(packet_received, "Must receive player packet");
    assert!(ticks_fired > 0, "60Hz tick must fire");
    println!("✓ Total 60Hz ticks fired during test: {}", ticks_fired);
    println!("=== TEST PASSED! ===\n");
}
