//! Temporary end-to-end smoke test for the UDP input hub.
//!
//! Run with:
//!   cargo run -p engine-server --example input_hub_smoke
//!
//! It boots the real host loop on loopback ports, registers two UDP peers, and
//! asserts that a validated opcode-19 frame reaches the other peer while a
//! duplicate-actor frame and a wrong-room frame do not. The host loop is
//! intentionally never joined: returning from `main` terminates the process.

use std::net::{SocketAddr, UdpSocket};
use std::time::{Duration, Instant};

use engine_config::{ControlConfig, GameConfig};
use engine_server::netcode::WireInputTick;

fn room_id() -> [u8; 8] {
    let mut id = [0u8; 8];
    id[..5].copy_from_slice(b"smoke");
    id
}

fn input(tick: u32, actor: u16) -> Vec<u8> {
    WireInputTick::to_packet_vec(room_id(), tick, actor, &[0u8; 9])
}

fn bind_client() -> UdpSocket {
    let s = UdpSocket::bind("127.0.0.1:0").expect("bind client");
    s.set_read_timeout(Some(Duration::from_millis(500)))
        .unwrap();
    s
}

fn main() {
    let server: SocketAddr = "127.0.0.1:39803".parse().unwrap();
    let cfg = GameConfig {
        name: "smoke".into(),
        port: 39801,
        web_port: 39802,
        tick_rate: 128,
        max_clients: 8,
        input_hub: true,
        control: ControlConfig {
            enabled: false,
            ..Default::default()
        },
        ..Default::default()
    };

    // SAFETY: single-threaded setup before the host thread starts; no other
    // thread reads or writes the environment concurrently.
    unsafe {
        std::env::set_var("OX_UDP_ADDR", server.to_string());
    }
    let project = std::path::PathBuf::from("/tmp/opencode/input_hub_smoke_project");
    std::fs::create_dir_all(&project).unwrap();
    let wasm = project.join("missing.wasm");
    let bus = engine_bus::Bus::new(64, 64);
    let cfg_for_thread = cfg.clone();
    let project_for_thread = project.clone();
    let wasm_for_thread = wasm.clone();
    let host = std::thread::spawn(move || {
        engine_server::host::run(&project_for_thread, &wasm_for_thread, &cfg_for_thread, bus)
    });

    // Give the host time to bind its sockets. The loop is otherwise idle.
    std::thread::sleep(Duration::from_millis(600));
    assert!(!host.is_finished(), "host exited during startup");

    let a = bind_client();
    let b = bind_client();

    // Register B first so A's frame has a UDP recipient. Each first packet is
    // also the peer's actor claim.
    b.send_to(&input(0, 2), server).unwrap();
    std::thread::sleep(Duration::from_millis(100));
    let _ = b.recv_from(&mut [0u8; 4096]); // ignore any unrelated datagram

    // A's valid frame must be forwarded to B exactly once.
    a.send_to(&input(0, 1), server).unwrap();
    let mut buf = [0u8; 4096];
    let (n, _) = b.recv_from(&mut buf).expect("B did not receive A's input");
    assert_eq!(n, 16 + 9);
    assert_eq!(buf[0], 19);
    assert_eq!(u16::from_be_bytes([buf[13], buf[14]]), 1);
    assert_eq!(u32::from_be_bytes([buf[9], buf[10], buf[11], buf[12]]), 0);

    // Drain any duplicate/late datagrams before the negative checks.
    b.set_read_timeout(Some(Duration::from_millis(250)))
        .unwrap();
    while b.recv_from(&mut buf).is_ok() {}

    // A third peer may not steal actor 1.
    let c = bind_client();
    c.send_to(&input(1, 1), server).unwrap();
    assert!(
        b.recv_from(&mut buf).is_err(),
        "duplicate actor claim was forwarded"
    );

    // A wrong room id is dropped before registration/forwarding.
    let mut wrong = input(1, 3);
    wrong[1..6].copy_from_slice(b"other");
    c.send_to(&wrong, server).unwrap();
    assert!(
        b.recv_from(&mut buf).is_err(),
        "wrong-room input was forwarded"
    );

    // A valid actor 3 frame from C must still work after the rejected ones.
    c.send_to(&input(1, 3), server).unwrap();
    let start = Instant::now();
    let mut saw_actor3 = false;
    while start.elapsed() < Duration::from_secs(2) {
        match b.recv_from(&mut buf) {
            Ok((n, _))
                if n == 25 && buf[0] == 19 && u16::from_be_bytes([buf[13], buf[14]]) == 3 =>
            {
                saw_actor3 = true;
                break;
            }
            _ => {}
        }
    }
    assert!(saw_actor3, "valid new actor's input was not forwarded");

    // Keep B alive while A and C time out (the host reaps after 3 s), then
    // prove A's actor was released and can be claimed again.
    while b.recv_from(&mut buf).is_ok() {}
    for tick in 1..=3 {
        std::thread::sleep(Duration::from_secs(1));
        b.send_to(&input(tick, 2), server).unwrap();
    }
    let d = bind_client();
    d.send_to(&input(0, 1), server).unwrap();
    let start = Instant::now();
    let mut reclaimed = false;
    while start.elapsed() < Duration::from_secs(2) {
        match b.recv_from(&mut buf) {
            Ok((n, _))
                if n == 25 && buf[0] == 19 && u16::from_be_bytes([buf[13], buf[14]]) == 1 =>
            {
                reclaimed = true;
                break;
            }
            _ => {}
        }
    }
    assert!(reclaimed, "timed-out actor 1 was not released for re-claim");

    println!("input_hub_smoke: PASS");
    std::process::exit(0);
}
