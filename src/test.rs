use std::collections::HashMap;
use std::fs::File;
use std::io::{self, Read};
use std::net::SocketAddr;
use std::path::Path;
use std::process::Command;

use mio::net::UdpSocket;
use mio::{Events, Interest, Poll, Token};

const SOCKET: Token = Token(0);
const MAX_DATAGRAM_SIZE: usize = 1350;

struct Client {
    conn: quiche::Connection,
    peer: SocketAddr,
}

type ClientMap = HashMap<quiche::ConnectionId<'static>, Client>;
type AliasMap = HashMap<quiche::ConnectionId<'static>, quiche::ConnectionId<'static>>;

/// Random server Connection ID generate karta hai.
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

/// Agar SSL certificates nahi hain to self-signed certs automatically create karta hai.
fn ensure_certificates() -> io::Result<()> {
    if !Path::new("cert.crt").exists() || !Path::new("cert.key").exists() {
        println!("Certificates nahi mile. Naye self-signed certificates generate ho rahe hain...");
        let status = Command::new("openssl")
            .args([
                "req",
                "-x509",
                "-newkey",
                "rsa:2048",
                "-nodes",
                "-keyout",
                "cert.key",
                "-out",
                "cert.crt",
                "-days",
                "365",
                "-subj",
                "/CN=localhost",
            ])
            .status()?;

        if !status.success() {
            return Err(io::Error::new(
                io::ErrorKind::Other,
                "OpenSSL certificate generation failed",
            ));
        }
        println!("cert.crt aur cert.key successfully generate ho gaye!");
    }
    Ok(())
}

fn main() -> io::Result<()> {
    // 1. SSL/TLS Certificates ensure karo
    ensure_certificates()?;

    // 2. MIO Poll aur Events setup
    let mut poll = Poll::new()?;
    let mut events = Events::with_capacity(1024);

    // 3. UDP Socket bind karo
    let bind_addr: SocketAddr = "127.0.0.1:4433".parse().unwrap();
    let mut socket = UdpSocket::bind(bind_addr)?;
    println!("QUIC Server listening on {}", socket.local_addr()?);

    poll.registry()
        .register(&mut socket, SOCKET, Interest::READABLE)?;

    // 4. QUIC Server Configuration
    let mut config = quiche::Config::new(quiche::PROTOCOL_VERSION).map_err(|e| {
        io::Error::new(io::ErrorKind::Other, format!("QUIC config error: {e}"))
    })?;

    config.load_cert_chain_from_pem_file("cert.crt").map_err(|e| {
        io::Error::new(io::ErrorKind::Other, format!("Cert load error: {e}"))
    })?;
    config.load_priv_key_from_pem_file("cert.key").map_err(|e| {
        io::Error::new(io::ErrorKind::Other, format!("Key load error: {e}"))
    })?;

    // Application protocols (ALPN)
    config
        .set_application_protos(&[b"openoxide-gaming", b"game-protocol", b"hq-interop", b"h3"])
        .map_err(|e| io::Error::new(io::ErrorKind::Other, format!("ALPN error: {e}")))?;

    config.set_max_idle_timeout(10000);
    config.set_max_recv_udp_payload_size(MAX_DATAGRAM_SIZE);
    config.set_max_send_udp_payload_size(MAX_DATAGRAM_SIZE);
    config.set_initial_max_data(10_000_000);
    config.set_initial_max_stream_data_bidi_local(1_000_000);
    config.set_initial_max_stream_data_bidi_remote(1_000_000);
    config.set_initial_max_stream_data_uni(1_000_000);
    config.set_initial_max_streams_bidi(100);
    config.set_initial_max_streams_uni(100);
    config.set_disable_active_migration(true);
    config.enable_early_data();

    // 5. Data structures & Buffers
    let mut recv_buf = [0u8; 65535];
    let mut send_buf = [0u8; MAX_DATAGRAM_SIZE];
    let mut clients = ClientMap::new();
    let mut conn_aliases = AliasMap::new();

    let local_addr = socket.local_addr()?;

    println!("Server loop start ho gaya. Client connections ke liye ready...");

    // 6. Main Event Loop
    loop {
        // Sabhi active connections ka shortest timeout calculate karo
        let timeout = clients.values().filter_map(|c| c.conn.timeout()).min();

        poll.poll(&mut events, timeout)?;

        // Agar koi I/O event nahi mila matlab timeout expire hua hai
        if events.is_empty() {
            for client in clients.values_mut() {
                client.conn.on_timeout();
            }
        }

        // Incoming packets read karne ke liye loop
        'read: loop {
            let (len, peer) = match socket.recv_from(&mut recv_buf) {
                Ok(result) => result,
                Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                    break 'read;
                }
                Err(e) => return Err(e),
            };

            let pkt_buf = &mut recv_buf[..len];

            // QUIC Packet Header parse karo
            let hdr = match quiche::Header::from_slice(pkt_buf, quiche::MAX_CONN_ID_LEN) {
                Ok(v) => v,
                Err(e) => {
                    eprintln!("Invalid packet header from {peer}: {e}");
                    continue 'read;
                }
            };

            // Connection lookup ya new connection creation
            let client = if !clients.contains_key(&hdr.dcid)
                && !conn_aliases.contains_key(&hdr.dcid)
            {
                // Agar new packet hai aur Initial nahi hai toh drop karo
                if hdr.ty != quiche::Type::Initial {
                    eprintln!("Packet from {peer} is not Initial, dropping");
                    continue 'read;
                }

                // Agar QUIC version supported nahi hai toh negotiate karo
                if !quiche::version_is_supported(hdr.version) {
                    println!("Unsupported version from {peer}, sending version negotiation");
                    let mut out = [0u8; MAX_DATAGRAM_SIZE];
                    if let Ok(n) = quiche::negotiate_version(&hdr.scid, &hdr.dcid, &mut out) {
                        let _ = socket.send_to(&out[..n], peer);
                    }
                    continue 'read;
                }

                // Naya Server Connection ID (SCID) generate karo
                let scid = generate_cid();
                println!("New connection accepted: peer={peer}, scid={scid:?}");

                let conn = match quiche::accept(&scid, None, local_addr, peer, &mut config) {
                    Ok(c) => c,
                    Err(e) => {
                        eprintln!("Failed to accept QUIC connection: {e}");
                        continue 'read;
                    }
                };

                let client = Client { conn, peer };

                // Client ka initial dcid save karo taaki subsequent packets map ho sakein
                conn_aliases.insert(hdr.dcid.clone().into_owned(), scid.clone());
                clients.insert(scid.clone(), client);

                clients.get_mut(&scid).unwrap()
            } else {
                // Existing connection fetch karo
                if let Some(target_scid) = conn_aliases.get(&hdr.dcid) {
                    clients.get_mut(target_scid).unwrap()
                } else {
                    clients.get_mut(&hdr.dcid).unwrap()
                }
            };

            // QUIC packet ko process karo
            let info = quiche::RecvInfo {
                from: peer,
                to: local_addr,
            };

            match client.conn.recv(pkt_buf, info) {
                Ok(_) => {}
                Err(quiche::Error::Done) => {}
                Err(e) => {
                    eprintln!("QUIC recv error: {e}");
                    continue 'read;
                }
            }

            // Game Data / Stream Processing
            if client.conn.is_established() || client.conn.is_in_early_data() {
                let readable: Vec<u64> = client.conn.readable().collect();

                for stream_id in readable {
                    let mut stream_buf = [0u8; 4096];
                    while let Ok((n, fin)) = client.conn.stream_recv(stream_id, &mut stream_buf) {
                        println!(
                            "[Stream {stream_id}] Received {n} bytes from {peer} (fin={fin}): {:?}",
                            String::from_utf8_lossy(&stream_buf[..n])
                        );

                        // Example echo response: Client ko wahi message wapas bhejna
                        let response = format!("Echo: {}", String::from_utf8_lossy(&stream_buf[..n]));
                        let _ = client
                            .conn
                            .stream_send(stream_id, response.as_bytes(), fin);
                    }
                }
            }
        }

        // Outgoing Packets Send Loop (Sabhi clients ke liye pending packets dispatch karo)
        for client in clients.values_mut() {
            loop {
                let (n, send_info) = match client.conn.send(&mut send_buf) {
                    Ok(res) => res,
                    Err(quiche::Error::Done) => break,
                    Err(e) => {
                        eprintln!("QUIC send packet error: {e}");
                        client.conn.close(false, 0x1, b"send fail").ok();
                        break;
                    }
                };

                if let Err(e) = socket.send_to(&send_buf[..n], send_info.to) {
                    if e.kind() == io::ErrorKind::WouldBlock {
                        break;
                    }
                    eprintln!("Socket send error to {}: {e}", send_info.to);
                    break;
                }
            }
        }

        // Garbage collection: Closed connections ko remove karo
        clients.retain(|scid, client| {
            if client.conn.is_closed() {
                println!("Connection closed: {scid:?} from {}", client.peer);
                false
            } else {
                true
            }
        });

        // Dead aliases clean up karo
        conn_aliases.retain(|_, target_scid| clients.contains_key(target_scid));
    }
}

