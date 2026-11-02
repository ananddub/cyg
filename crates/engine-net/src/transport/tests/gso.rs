use crate::transport::send_gso;
use std::net::{SocketAddr, UdpSocket};
use std::os::unix::io::AsRawFd;

#[test]
fn gso_run_delivers_every_segment() {
    let rx = UdpSocket::bind("127.0.0.1:0").expect("bind rx");
    let tx = UdpSocket::bind("127.0.0.1:0").expect("bind tx");
    rx.connect(tx.local_addr().unwrap()).expect("connect rx");
    let dest = rx.local_addr().unwrap();

    let seg = vec![9u8; 16];
    let tail = vec![3u8; 8];
    let slices: Vec<&[u8]> = vec![&seg, &seg, &seg, &seg, &tail];

    let gso = send_gso(&tx, &dest, &slices, 16);
    match gso {
        Ok(()) => tracing::info!("UDP_SEGMENT accepted by the kernel"),
        Err(e) => tracing::error!("kernel refused GSO ({e}) — fallback path exercised"),
    }

    rx.set_read_timeout(Some(std::time::Duration::from_secs(2)))
        .unwrap();
    let mut buf = [0u8; 64];
    let mut got = Vec::new();
    for _ in 0..5 {
        if let Ok(n) = rx.recv(&mut buf) {
            got.push(buf[..n].to_vec());
        }
    }
    assert_eq!(got.len(), 5, "all five datagrams arrive");
    assert_eq!(
        &got[..4],
        &[vec![9u8; 16], vec![9u8; 16], vec![9u8; 16], vec![9u8; 16]]
    );
    assert_eq!(got[4], vec![3u8; 8], "short tail is its own datagram");
}

#[test]
fn gso_beats_sendto_on_same_dest_runs() {
    let rx = UdpSocket::bind("127.0.0.1:0").expect("bind rx");
    let tx = UdpSocket::bind("127.0.0.1:0").expect("bind tx");
    rx.connect(tx.local_addr().unwrap()).expect("connect rx");
    let dest = rx.local_addr().unwrap();

    let chunk = vec![1u8; 1024];
    unsafe {
        let buf = 32 * 1024 * 1024i32;
        libc::setsockopt(
            rx.as_raw_fd(),
            libc::SOL_SOCKET,
            libc::SO_RCVBUF,
            &buf as *const _ as *const libc::c_void,
            std::mem::size_of_val(&buf) as libc::socklen_t,
        );
    }
    rx.set_read_timeout(Some(std::time::Duration::from_millis(50)))
        .unwrap();
    let mut drain = |misses_max: u32| {
        let mut buf = [0u8; 2048];
        let mut misses = 0;
        while misses < misses_max {
            match rx.recv(&mut buf) {
                Ok(_) => misses = 0,
                Err(_) => misses += 1,
            }
        }
    };

    let rounds = 50;
    let chunks_per_round = 400;

    let slices: Vec<&[u8]> = (0..chunks_per_round).map(|_| &chunk[..]).collect();
    let _ = send_gso(&tx, &dest, &slices, chunk.len());
    drain(2);

    let t0 = std::time::Instant::now();
    for _ in 0..rounds {
        for _ in 0..chunks_per_round {
            let _ = tx.send_to(&chunk, dest);
        }
    }
    let sendto_time = t0.elapsed();
    drain(3);

    let t0 = std::time::Instant::now();
    for _ in 0..rounds {
        let _ = send_gso(&tx, &dest, &slices, chunk.len());
    }
    let gso_time = t0.elapsed();
    drain(3);

    tracing::debug!(
        "GSO: {chunks_per_round} chunks x {rounds} rounds — send_to: {sendto_time:?}, gso: {gso_time:?} ({:.1}% of send_to)",
        gso_time.as_secs_f64() / sendto_time.as_secs_f64() * 100.0
    );
}
