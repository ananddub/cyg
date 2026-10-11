use std::fs::File;
use std::io::{self, Read};
use std::net::SocketAddr;

use mio::net::UdpSocket;
use mio::{Events, Interest, Poll, Token};

const SOCKET: Token = Token(0);
const MAX_DATAGRAM_SIZE: usize = 1350;

fn generate_cid() -> quiche::ConnectionId<'static> {
    let mut bytes = [0u8; quiche::MAX_CONN_ID_LEN];
    if let Ok(mut f) = File::open("/dev/urandom") {
        let _ = f.read_exact(&mut bytes);
    } else {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        for (i, b) in bytes.iter_mut().enumerate() {
            *b = (now >> (i * 8)) as u8;
        }
    }
    quiche::ConnectionId::from_vec(bytes.to_vec())
}

fn main() -> io::Result<()> {
    println!("=== OpenOxide Raw QUIC Game Client ===");

    // 1. MIO Event Loop setup
    let mut poll = Poll::new()?;
    let mut events = Events::with_capacity(1024);

    // 2. Client UDP Socket bind
    let mut socket = UdpSocket::bind("127.0.0.1:0".parse().unwrap())?;
    let server_addr: SocketAddr = "127.0.0.1:4433".parse().unwrap();
    let local_addr = socket.local_addr()?;

    poll.registry()
        .register(&mut socket, SOCKET, Interest::READABLE)?;

    // 3. Client QUIC Configuration
    let mut config = quiche::Config::new(quiche::PROTOCOL_VERSION).map_err(|e| {
        io::Error::new(io::ErrorKind::Other, format!("Config error: {e}"))
    })?;

    // Self-signed test certificate ke liye peer verification disable
    config.verify_peer(false);

    // Game ALPN protocol
    config
        .set_application_protos(&[b"openoxide-gaming", b"game-protocol"])
        .map_err(|e| io::Error::new(io::ErrorKind::Other, format!("ALPN error: {e}")))?;

    config.set_max_idle_timeout(5000);
    config.set_max_recv_udp_payload_size(MAX_DATAGRAM_SIZE);
    config.set_max_send_udp_payload_size(MAX_DATAGRAM_SIZE);
    config.set_initial_max_data(10_000_000);
    config.set_initial_max_stream_data_bidi_local(1_000_000);
    config.set_initial_max_stream_data_bidi_remote(1_000_000);
    config.set_initial_max_streams_bidi(100);
    config.set_initial_max_streams_uni(100);

    // 4. Client Connection ID & Connect call
    let scid = generate_cid();
    let mut conn = quiche::connect(None, &scid, local_addr, server_addr, &mut config)
        .map_err(|e| io::Error::new(io::ErrorKind::Other, format!("Connect error: {e}")))?;

    println!("Connecting to server {server_addr} from {local_addr}...");

    let mut recv_buf = [0u8; 65535];
    let mut send_buf = [0u8; MAX_DATAGRAM_SIZE];
    let mut message_sent = false;

    // Initial packet send
    let (n, send_info) = conn.send(&mut send_buf).expect("Initial send failed");
    socket.send_to(&send_buf[..n], send_info.to)?;

    // 5. Client Event Loop
    loop {
        poll.poll(&mut events, conn.timeout())?;

        if events.is_empty() {
            conn.on_timeout();
        }

        // Read UDP packets from Server
        'read: loop {
            let (len, peer) = match socket.recv_from(&mut recv_buf) {
                Ok(v) => v,
                Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => break 'read,
                Err(e) => return Err(e),
            };

            let recv_info = quiche::RecvInfo {
                from: peer,
                to: local_addr,
            };

            match conn.recv(&mut recv_buf[..len], recv_info) {
                Ok(_) => {}
                Err(quiche::Error::Done) => {}
                Err(e) => {
                    eprintln!("QUIC recv error: {e}");
                    break 'read;
                }
            }
        }

        // Check Handshake status & send Game Stream data
        if conn.is_established() && !message_sent {
            println!("QUIC TLS 1.3 Handshake COMPLETE! Connected to server.");

            // Raw game stream 0 par message bhejna
            let game_payload = b"PLAYER_INPUT: id=42, action=SPRINT, pos=[12.5, 4.0, 99.1]";
            let stream_id = 0;

            match conn.stream_send(stream_id, game_payload, true) {
                Ok(n) => {
                    println!("[Client -> Server] Sent {n} bytes on stream {stream_id}: {:?}", String::from_utf8_lossy(game_payload));
                    message_sent = true;
                }
                Err(e) => {
                    eprintln!("Failed to send stream data: {e}");
                }
            }
        }

        // Read Server's Stream Response
        if conn.is_established() {
            let readable: Vec<u64> = conn.readable().collect();
            for s in readable {
                let mut buf = [0u8; 4096];
                while let Ok((n, fin)) = conn.stream_recv(s, &mut buf) {
                    println!(
                        "[Server -> Client] Received on stream {s}: {:?}",
                        String::from_utf8_lossy(&buf[..n])
                    );

                    if fin {
                        println!("Server finished stream response. Test successful!");
                        conn.close(true, 0x0, b"done").ok();
                    }
                }
            }
        }

        // Flush pending outgoing packets
        loop {
            let (n, send_info) = match conn.send(&mut send_buf) {
                Ok(v) => v,
                Err(quiche::Error::Done) => break,
                Err(e) => {
                    eprintln!("Send error: {e}");
                    break;
                }
            };

            if let Err(e) = socket.send_to(&send_buf[..n], send_info.to) {
                if e.kind() == io::ErrorKind::WouldBlock {
                    break;
                }
                eprintln!("Socket send error: {e}");
                break;
            }
        }

        if conn.is_closed() {
            println!("Connection closed gracefully.");
            break;
        }
    }

    Ok(())
}
