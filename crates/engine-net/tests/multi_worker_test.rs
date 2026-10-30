use std::net::TcpStream;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::thread;
use std::time::Duration;

use engine_net::{PoolConfig, spawn_worker_pool};

#[test]
fn test_multi_worker_pool_so_reuseport() {
    let port = 19195;
    let bind_addr = format!("127.0.0.1:{}", port).parse().unwrap();
    let tick_count = Arc::new(AtomicU32::new(0));
    let tick_clone = tick_count.clone();

    let pool_cfg = PoolConfig {
        bind_addr,
        worker_count: 2,
        tick_rate: 60,
        max_clients_per_worker: 64,
    };

    let pool_handle = spawn_worker_pool(pool_cfg, move |_worker_id| {
        tick_clone.fetch_add(1, Ordering::Relaxed);
    })
    .expect("Failed to spawn multi-worker pool");

    // Allow workers to boot and bind
    thread::sleep(Duration::from_millis(100));

    // Connect 2 clients across the SO_REUSEPORT port
    let client1 = TcpStream::connect(bind_addr).expect("Client 1 connect ok");
    let client2 = TcpStream::connect(bind_addr).expect("Client 2 connect ok");
    client1.set_nodelay(true).unwrap();
    client2.set_nodelay(true).unwrap();

    // Sleep a bit so ticks accumulate
    thread::sleep(Duration::from_millis(150));

    let telemetry = pool_handle.telemetry();
    println!("Aggregated telemetry: {:?}", telemetry);

    assert!(
        telemetry.total_ticks > 0,
        "Workers should have advanced ticks"
    );
    assert_eq!(
        telemetry.total_ccu, 2,
        "Both client connections should be registered in CCU"
    );

    // Stop pool gracefully
    pool_handle.stop();
    pool_handle.join().expect("Pool shutdown cleanly");
}
