use crate::transport::send_datagram_batch;
use std::net::{SocketAddr, UdpSocket};
use std::os::unix::io::AsRawFd;

#[test]
fn batch_delivers_every_datagram() {
    let rx = UdpSocket::bind("127.0.0.1:0").expect("bind rx");
    let tx = UdpSocket::bind("127.0.0.1:0").expect("bind tx");
    rx.connect(tx.local_addr().unwrap()).expect("connect rx");

    let msgs: Vec<(SocketAddr, Vec<u8>)> = (0..8u32)
        .map(|i| {
            (
                rx.local_addr().unwrap(),
                Vec::from(
                    (i * 111..i * 111 + 16)
                        .map(|b| b as u8)
                        .collect::<Vec<u8>>(),
                ),
            )
        })
        .collect();

    let sent = send_datagram_batch(&tx, &msgs);
    assert_eq!(sent, 8);

    rx.set_read_timeout(Some(std::time::Duration::from_secs(2)))
        .unwrap();
    let mut buf = [0u8; 64];
    for i in 0..8u32 {
        let n = rx.recv(&mut buf).expect("datagram arrives");
        assert_eq!(
            &buf[..n],
            &(i * 111..i * 111 + 16)
                .map(|b| b as u8)
                .collect::<Vec<u8>>()
        );
    }
}

#[test]
fn batch_beats_sendto_walltime_on_loopback() {
    let rx = UdpSocket::bind("127.0.0.1:0").expect("bind rx");
    let tx = UdpSocket::bind("127.0.0.1:0").expect("bind tx");
    rx.connect(tx.local_addr().unwrap()).expect("connect rx");
    let dest = rx.local_addr().unwrap();
    let payload = vec![7u8; 64];
    let msgs: Vec<(SocketAddr, Vec<u8>)> = (0..4096).map(|_| (dest, payload.clone())).collect();

    unsafe {
        let buf = 16 * 1024 * 1024i32;
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

    let mut drain = |expected: usize| {
        let mut buf = [0u8; 96];
        let mut got = 0;
        let mut misses = 0;
        while got < expected && misses < 4 {
            match rx.recv(&mut buf) {
                Ok(_) => {
                    got += 1;
                    misses = 0;
                }
                Err(_) => misses += 1,
            }
        }
        got
    };

    let _ = send_datagram_batch(&tx, &msgs);
    drain(msgs.len());

    let rounds = 3;
    let t0 = std::time::Instant::now();
    for _ in 0..rounds {
        for (addr, data) in &msgs {
            let _ = tx.send_to(data, addr);
        }
    }
    let sendto_time = t0.elapsed();
    let _ = drain(rounds * msgs.len());

    let t0 = std::time::Instant::now();
    for _ in 0..rounds {
        send_datagram_batch(&tx, &msgs);
    }
    let batch_time = t0.elapsed();
    let _ = drain(rounds * msgs.len());

    tracing::info!(
        "sendmmsg: 4096 msgs x {rounds} rounds — send_to: {sendto_time:?}, batch: {batch_time:?} ({:.1}% of send_to)",
        batch_time.as_secs_f64() / sendto_time.as_secs_f64() * 100.0
    );
    assert!(
        batch_time.as_secs_f64() <= sendto_time.as_secs_f64() * 1.2,
        "batching must not be meaningfully slower than per-message sends"
    );
}
