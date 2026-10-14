//! End-to-end WebTransport tests: a real quiche client against our server,
//! exercising extended CONNECT, WT bidi streams and datagrams.

use std::collections::HashMap;
use std::net::UdpSocket;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use bytes::BytesMut;

use engine_webtransport::varint::encode_into;
use engine_webtransport::{WtConfig, WtEvent, WtRequest, WtResponse, WebTransportServer};

const CERT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../cert.crt");
const KEY: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../cert.key");

struct WtTestClient {
    sock: UdpSocket,
    conn: quiche::Connection,

    streams: HashMap<u64, Vec<u8>>,
    dgrams: Vec<Vec<u8>>,
}

impl WtTestClient {
    fn connect(peer: std::net::SocketAddr) -> Self {
        let mut config = quiche::Config::new(quiche::PROTOCOL_VERSION).unwrap();
        config.set_application_protos(&[b"h3"]).unwrap();
        config.verify_peer(false);
        config.set_max_idle_timeout(4000);
        config.enable_dgram(true, 64, 64);
        config.set_initial_max_data(64 * 1024 * 1024);
        config.set_initial_max_stream_data_bidi_local(4 * 1024 * 1024);
        config.set_initial_max_stream_data_bidi_remote(4 * 1024 * 1024);
        config.set_initial_max_stream_data_uni(4 * 1024 * 1024);
        config.set_initial_max_streams_bidi(16);
        config.set_initial_max_streams_uni(16);

        let scid = [1u8, 2, 3, 4, 5, 6, 7, 8];
        let scid = quiche::ConnectionId::from_ref(&scid);
        let sock = UdpSocket::bind("127.0.0.1:0").unwrap();
        sock.connect(peer).unwrap();
        let local = sock.local_addr().unwrap();
        let conn = quiche::connect(Some("localhost"), &scid, local, peer, &mut config).unwrap();
        let mut me = Self {
            sock,
            conn,

            streams: HashMap::new(),
            dgrams: Vec::new(),
        };
        me.flush();
        me
    }

    fn flush(&mut self) {
        let mut out = vec![0u8; 65536];
        loop {
            match self.conn.send(&mut out) {
                Ok((n, _info)) => {
                    let _ = self.sock.send(&out[..n]);
                }
                Err(_) => break,
            }
        }
    }

    fn send_stream(&mut self, id: u64, data: &[u8], fin: bool) {
        let _ = self.conn.stream_send(id, data, fin);
        self.flush();
    }

    /// Pump IO for up to `budget`, accumulating stream bytes and datagrams.
    fn pump(&mut self, budget: Duration) {
        let deadline = Instant::now() + budget;
        self.sock
            .set_read_timeout(Some(Duration::from_millis(20)))
            .unwrap();
        let mut buf = vec![0u8; 65536];
        while Instant::now() < deadline {
            match self.sock.recv_from(&mut buf) {
                Ok((n, from)) => {
                    let info = quiche::RecvInfo {
                        from,
                        to: self.sock.local_addr().unwrap(),
                    };
                    let _ = self.conn.recv(&mut buf[..n], info);
                }
                Err(ref e)
                    if e.kind() == std::io::ErrorKind::WouldBlock
                        || e.kind() == std::io::ErrorKind::TimedOut =>
                {
                    self.conn.on_timeout();
                }
                Err(_) => break,
            }
            let readable: Vec<u64> = self.conn.readable().collect();
            let mut tmp = vec![0u8; 16384];
            for id in readable {
                loop {
                    match self.conn.stream_recv(id, &mut tmp) {
                        Ok((0, _)) => break,
                        Ok((n, _)) => self.streams.entry(id).or_default().extend_from_slice(&tmp[..n]),
                        Err(_) => break,
                    }
                }
            }
            let mut dbuf = vec![0u8; 65536];
            while let Ok(n) = self.conn.dgram_recv(&mut dbuf) {
                self.dgrams.push(dbuf[..n].to_vec());
            }
            self.flush();
        }
    }

    fn wait_for<T>(
        &mut self,
        timeout: Duration,
        mut check: impl FnMut(&mut Self) -> Option<T>,
    ) -> Option<T> {
        let deadline = Instant::now() + timeout;
        while Instant::now() < deadline {
            if let Some(v) = check(self) {
                return Some(v);
            }
            self.pump(Duration::from_millis(30));
        }
        check(self)
    }
}

fn client_settings_frame() -> Vec<u8> {
    let mut payload = Vec::new();
    let mut kv = |k: u64, v: u64| {
        encode_into(k, &mut payload);
        encode_into(v, &mut payload);
    };
    kv(0x01, 0); // qpack max table capacity
    kv(0x06, 0); // qpack blocked streams
    kv(0x03, 65536); // max field section size
    kv(0x08, 1); // enable_connect_protocol
    kv(0x2b, 1); // enable_webtransport
    kv(0x33, 1); // h3_datagram
    let mut out = Vec::new();
    encode_into(0u64, &mut out); // uni control stream type
    let mut f = Vec::new();
    encode_into(0x04u64, &mut f); // SETTINGS frame type
    encode_into(payload.len() as u64, &mut f);
    f.extend_from_slice(&payload);
    out.extend_from_slice(&f);
    out
}

fn headers_frame(fields: Vec<engine_webtransport::qpack::HeaderField>) -> Vec<u8> {
    let mut block = BytesMut::new();
    engine_webtransport::qpack::encode_stateless(&mut block, fields.iter()).unwrap();
    let mut f = Vec::new();
    encode_into(0x01u64, &mut f); // HEADERS
    encode_into(block.len() as u64, &mut f);
    f.extend_from_slice(&block);
    f
}

#[test]
fn full_session_stream_and_datagram_echo() {
    let stop = Arc::new(AtomicBool::new(false));
    let srv = WebTransportServer::new(WtConfig {
        bind: "127.0.0.1:0".into(),
        cert: CERT.into(),
        key: KEY.into(),
        max_idle_timeout: Duration::from_secs(5),
    })
    .expect("server bind");
    let addr = srv.local_addr();

    let stop_c = stop.clone();
    std::thread::spawn(move || {
        let mut srv = srv;
        let mut events: Vec<WtEvent> = Vec::new();
        while !stop_c.load(Ordering::Relaxed) {
            srv.poll_events(Duration::from_millis(50), &mut events).unwrap();
            for ev in events.drain(..) {
                match ev {
                    WtEvent::SessionOpened { session, .. } => {
                        srv.accept_session(session).unwrap();
                    }
                    WtEvent::StreamData {
                        session,
                        stream,
                        data,
                        ..
                    } => {
                        srv.stream_send(session, stream, &data, false).unwrap();
                    }
                    WtEvent::Datagram { session, data } => {
                        srv.send_datagram(session, &data).unwrap();
                    }
                    _ => {}
                }
            }
        }
    });

    let mut cl = WtTestClient::connect(addr);
    assert!(
        cl.wait_for(Duration::from_secs(3), |cl| cl.conn.is_established().then_some(()))
            .is_some(),
        "QUIC handshake did not complete"
    );
    // Control stream (client uni id 1) with our SETTINGS.
    cl.send_stream(1, &client_settings_frame(), false);

    // Extended CONNECT on bidi stream 0.
    let req = WtRequest::webtransport(addr.to_string(), "/game/lobby")
        .header("sec-webtransport-session-id", "lobby-7");
    cl.send_stream(0, &headers_frame(req.to_fields()), false);

    // Wait for :status: 200 response on stream 0.
    let status = cl.wait_for(Duration::from_secs(4), |cl| {
        let buf = cl.streams.get(&0u64)?;
        let (kind, payload, _) =
            engine_webtransport::h3::try_parse_frame(buf).ok()?;
        if kind != 1 {
            return None;
        }
        let mut cur = bytes::Bytes::copy_from_slice(payload);
        let d = engine_webtransport::qpack::decode_stateless(&mut cur, 65536).ok()?;
        let resp = WtResponse::from_fields(&d.fields).ok()?;
        Some(resp)
    });
    let resp = status.expect("no CONNECT response");
    assert_eq!(resp.code(), 200);
    assert_eq!(
        resp.headers().iter().find(|(k, _)| k == "sec-webtransport-session-id").map(|(_, v)| v.as_str()),
        Some("lobby-7"),
        "session id tag should echo"
    );

    // WT bidi stream: client bidi id 4 with prefix 0x41 + session id 0.
    let mut wt = Vec::new();
    encode_into(0x41u64, &mut wt);
    encode_into(0u64, &mut wt);
    wt.extend_from_slice(b"hello-wt");
    cl.send_stream(4, &wt, false);

    let echoed = cl.wait_for(Duration::from_secs(3), |cl| {
        let buf = cl.streams.get(&4u64)?;
        if buf.starts_with(b"hello-wt") {
            Some(buf.clone())
        } else {
            None
        }
    });
    assert!(echoed.is_some(), "no echo on WT bidi stream");

    // WT datagram: record = varint(len) varint(session=0) payload.
    let mut content = Vec::new();
    encode_into(0u64, &mut content);
    content.extend_from_slice(b"hello-dg");
    let mut rec = Vec::new();
    encode_into(content.len() as u64, &mut rec);
    rec.extend_from_slice(&content);
    cl.conn.dgram_send(&rec).unwrap();
    cl.flush();

    let dgram = cl.wait_for(Duration::from_secs(3), |cl| {
        cl.dgrams.iter().find_map(|d| {
            // parse first record's payload
            let mut cur: &[u8] = d;
            let (rlen, n1) = engine_webtransport::varint::decode(cur)?;
            cur = &cur[n1..];
            let body = cur.get(..rlen as usize)?;
            let (_sid, n2) = engine_webtransport::varint::decode(body)?;
            if &body[n2..] == b"hello-dg" {
                Some(body[n2..].to_vec())
            } else {
                None
            }
        })
    });
    assert!(dgram.is_some(), "no datagram echo");

    stop.store(true, Ordering::Relaxed);
}

#[test]
fn get_probe_advertises_draft02() {
    let srv = WebTransportServer::new(WtConfig {
        bind: "127.0.0.1:0".into(),
        cert: CERT.into(),
        key: KEY.into(),
        max_idle_timeout: Duration::from_secs(5),
    })
    .expect("server bind");
    let addr = srv.local_addr();
    let stop = Arc::new(AtomicBool::new(false));
    let stop_c = stop.clone();
    std::thread::spawn(move || {
        let mut srv = srv;
        let mut events: Vec<WtEvent> = Vec::new();
        while !stop_c.load(Ordering::Relaxed) {
            let _ = srv.poll_events(Duration::from_millis(50), &mut events);
            events.clear();
        }
    });

    let mut cl = WtTestClient::connect(addr);
    assert!(
        cl.wait_for(Duration::from_secs(3), |cl| cl.conn.is_established().then_some(()))
            .is_some(),
        "QUIC handshake did not complete"
    );
    cl.send_stream(1, &client_settings_frame(), false);
    let req = WtRequest::get(addr.to_string(), "/");
    cl.send_stream(0, &headers_frame(req.to_fields()), true);

    let resp = cl.wait_for(Duration::from_secs(4), |cl| {
        let buf = cl.streams.get(&0u64)?;
        let (kind, payload, _) =
            engine_webtransport::h3::try_parse_frame(buf).ok()?;
        if kind != 1 {
            return None;
        }
        let mut cur = bytes::Bytes::copy_from_slice(payload);
        let d = engine_webtransport::qpack::decode_stateless(&mut cur, 65536).ok()?;
        WtResponse::from_fields(&d.fields).ok()
    });
    let resp = resp.expect("no probe response");
    assert_eq!(resp.code(), 200);
    assert!(resp
        .headers()
        .iter()
        .any(|(k, v)| k == "sec-webtransport-http3-draft" && v == "draft02"));

    stop.store(true, Ordering::Relaxed);
}
