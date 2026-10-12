//! Raw-mio UDP + quiche event loop hosting WebTransport-over-HTTP/3.
//!
//! No async runtime: one `Poll`, SO_REUSEPORT-friendly UDP socket, and a
//! per-connection state machine implementing the sliver of HTTP/3 that
//! WebTransport needs (extended CONNECT, WT stream prefixes 0x41/0x51/0x54,
//! length-prefixed datagram records, close capsules).
//!
//! QPACK posture: we advertise `SETTINGS_QPACK_MAX_TABLE_CAPACITY = 0` and
//! blocked-streams 0, so spec-compliant clients must emit only static/literal
//! forms — `qpack::encode_stateless`/`decode_stateless` is then fully valid
//! and we never implement the dynamic table.

use std::collections::{HashMap, VecDeque};
use std::hash::{Hash, Hasher};
use std::io;
use std::net::{SocketAddr, UdpSocket as StdUdpSocket};
use std::time::{Duration, Instant};

use bytes::{Bytes, BytesMut};
use mio::net::UdpSocket as MioUdpSocket;
use mio::{Events, Interest, Poll, Token, Waker};
use qpack::{decode_stateless, encode_stateless};

use crate::h3::{self, frame_type, stream_type};
use crate::headers::{WtRequest, WtResponse, WT_SESSION_ID_HEADER};
use crate::varint::{self, encode_into};

const UDP: Token = Token(0);
const WAKER: Token = Token(1);
const READ_BUF_LEN: usize = 65536;
const STREAM_CHUNK: usize = 16384;
const MAX_HEADER_BLOCK: usize = 64 * 1024;
const SERVER_ALPN: &[u8] = b"h3";

/// Server knobs. `cert`/`key` are PEM paths (self-signed is fine locally;
/// browsers require a trusted cert in production).
#[derive(Debug, Clone)]
pub struct WtConfig {
    pub bind: String,
    pub cert: String,
    pub key: String,
    pub max_idle_timeout: Duration,
}

impl Default for WtConfig {
    fn default() -> Self {
        Self {
            bind: "127.0.0.1:4443".into(),
            cert: "cert.crt".into(),
            key: "cert.key".into(),
            max_idle_timeout: Duration::from_secs(30),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamDir {
    Bidi,
    UniClient,
    UniServer,
}

/// Application-visible events, same drain style as `WsServer::poll_events`.
#[derive(Debug)]
pub enum WtEvent {
    /// Client opened a WT session via extended CONNECT. Respond with
    /// [`WebTransportServer::accept_session`] or `reject_session` before
    /// using the session.
    SessionOpened { session: u64, request: WtRequest },
    /// Session closed, by either peer.
    SessionClosed {
        session: u64,
        code: u64,
        reason: String,
    },
    /// A WebTransport stream of the session was opened by the client.
    StreamOpened {
        session: u64,
        stream: u64,
        dir: StreamDir,
    },
    StreamData {
        session: u64,
        stream: u64,
        data: Vec<u8>,
        fin: bool,
    },
    /// Decoded WT datagram payload for the session.
    Datagram { session: u64, data: Vec<u8> },
    /// The whole QUIC connection ended.
    ConnectionClosed { reason: String },
}

/// What a receive stream buffer is doing.
enum StreamIn {
    /// First varint(s) not yet complete: accumulating. `bidi` tells which
    /// classification table to apply once we can decode.
    Classify { buf: Vec<u8>, bidi: bool },
    Control { buf: Vec<u8> },
    Qpack,
    Request {
        buf: Vec<u8>,
        request: Option<WtRequest>,
        terminal: bool,
    },
    /// Active WebTransport data stream of `session` handle.
    Wt { session: u64 },
}

struct WriteChunk {
    data: Vec<u8>,
    pos: usize,
    fin: bool,
}

struct EngineConn {
    conn: quiche::Connection,
    peer: SocketAddr,
    control_written: bool,
    peer_settings: Option<h3::PeerSettings>,
    streams: HashMap<u64, StreamIn>,
    pending: HashMap<u64, VecDeque<WriteChunk>>,
    next_server_uni: u64,
}

struct SessionRec {
    cidx: u64,
    request_stream: u64,
    request: WtRequest,
    accepted: bool,
    closed: bool,
}

/// WebTransport server bound to one UDP socket.
pub struct WebTransportServer {
    poll: Poll,
    waker: Waker,
    socket: MioUdpSocket,
    local: SocketAddr,
    qconfig: quiche::Config,
    conns: HashMap<u64, EngineConn>,
    conn_by_dcid: HashMap<Vec<u8>, u64>,
    next_cidx: u64,
    sessions: HashMap<u64, SessionRec>,
    handle_by_stream: HashMap<(u64, u64), u64>,
    next_handle: u64,
    events: Events,
}

impl WebTransportServer {
    pub fn new(cfg: WtConfig) -> io::Result<Self> {
        let bind: SocketAddr = cfg
            .bind
            .parse()
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?;

        let mut qconfig = quiche::Config::new(quiche::PROTOCOL_VERSION)
            .map_err(|e| io::Error::other(format!("quiche config: {e}")))?;
        qconfig
            .load_cert_chain_from_pem_file(&cfg.cert)
            .map_err(|e| io::Error::other(format!("load cert {}: {e}", cfg.cert)))?;
        qconfig
            .load_priv_key_from_pem_file(&cfg.key)
            .map_err(|e| io::Error::other(format!("load key {}: {e}", cfg.key)))?;
        qconfig
            .set_application_protos(&[SERVER_ALPN])
            .map_err(|e| io::Error::other(format!("alpn: {e}")))?;
        qconfig.set_max_idle_timeout(cfg.max_idle_timeout.as_millis() as u64);
        qconfig.verify_peer(false);
        qconfig.enable_dgram(true, 64, 64);
        qconfig.set_initial_max_data(64 * 1024 * 1024);
        qconfig.set_initial_max_stream_data_bidi_local(4 * 1024 * 1024);
        qconfig.set_initial_max_stream_data_bidi_remote(4 * 1024 * 1024);
        qconfig.set_initial_max_stream_data_uni(4 * 1024 * 1024);
        qconfig.set_initial_max_streams_bidi(1024);
        qconfig.set_initial_max_streams_uni(1024);

        let std_sock: StdUdpSocket = StdUdpSocket::bind(bind)?;
        std_sock.set_nonblocking(true)?;
        let mut socket = MioUdpSocket::from_std(std_sock);
        let local = socket.local_addr()?;

        let poll = Poll::new()?;
        let waker = Waker::new(poll.registry(), WAKER)?;
        poll.registry()
            .register(&mut socket, UDP, Interest::READABLE)?;

        Ok(Self {
            poll,
            waker,
            socket,
            local,
            qconfig,
            conns: HashMap::new(),
            conn_by_dcid: HashMap::new(),
            next_cidx: 0,
            sessions: HashMap::new(),
            handle_by_stream: HashMap::new(),
            next_handle: 0,
            events: Events::with_capacity(128),
        })
    }

    pub fn local_addr(&self) -> SocketAddr {
        self.local
    }

    pub fn waker(&self) -> &Waker {
        &self.waker
    }

    pub fn session_count(&self) -> usize {
        self.sessions.values().filter(|s| !s.closed).count()
    }

    /// Drive the QUIC/h3/WT machinery for up to `wait`, draining app events.
    pub fn poll_events(&mut self, wait: Duration) -> io::Result<Vec<WtEvent>> {
        let mut out = Vec::new();
        let now = Instant::now();

        let mut timeout = wait;
        for c in self.conns.values() {
            if let Some(t) = c.conn.timeout()
                && t < timeout
            {
                timeout = t;
            }
        }

        self.poll.poll(&mut self.events, Some(timeout))?;
        let fired = !self.events.is_empty();
        let udp_fired = self.events.iter().any(|e| e.token() == UDP);

        if udp_fired {
            loop {
                let mut buf = vec![0u8; READ_BUF_LEN];
                match self.socket.recv_from(&mut buf) {
                    Ok((len, from)) => {
                        buf.truncate(len);
                        self.route_packet(buf, from, now, &mut out);
                    }
                    Err(ref e)
                        if e.kind() == io::ErrorKind::WouldBlock
                            || e.kind() == io::ErrorKind::Interrupted =>
                    {
                        break
                    }
                    Err(e) => return Err(e),
                }
            }
        }

        if !fired {
            for c in self.conns.values_mut() {
                c.conn.on_timeout();
            }
        }

        // Drain application writes that were flow/blocked earlier, then push
        // every connection's packets out.
        let cids: Vec<u64> = self.conns.keys().copied().collect();
        for cidx in cids {
            self.flush_pending(cidx);
            self.flush_send(cidx, &mut out);
        }

        self.reap_closed(&mut out);
        Ok(out)
    }

    fn route_packet(&mut self, mut packet: Vec<u8>, from: SocketAddr, now: Instant, out: &mut Vec<WtEvent>) {
        let (dcid, scid, ty, version) = match quiche::Header::from_slice(&mut packet, quiche::MAX_CONN_ID_LEN) {
            Ok(h) => (
                h.dcid.as_ref().to_vec(),
                h.scid.as_ref().to_vec(),
                h.ty,
                h.version,
            ),
            Err(_) => return,
        };

        let known = self.conn_by_dcid.get(&dcid).copied();
        if let Some(cidx) = known {
            let info = quiche::RecvInfo {
                from,
                to: self.local,
            };
            {
                let Some(c) = self.conns.get_mut(&cidx) else {
                    return;
                };
                let _ = c.conn.recv(&mut packet, info);
                c.peer = from;
            }
            self.process_conn(cidx, now, out);
            return;
        }

        // Retry-token-free path: unknown CID.
        if ty == quiche::Type::Initial && quiche::version_is_supported(version) {
            self.new_conn(&dcid, &scid, packet, from, now, out);
        } else if version != quiche::PROTOCOL_VERSION && ty != quiche::Type::Short {
            let mut outb = vec![0u8; READ_BUF_LEN];
            let scid_cid = quiche::ConnectionId::from_ref(&scid);
            let dcid_cid = quiche::ConnectionId::from_ref(&dcid);
            if let Ok(len) = quiche::negotiate_version(&scid_cid, &dcid_cid, &mut outb) {
                let _ = self.socket.send_to(&outb[..len], from);
            }
        } else {
            // Mid-connection packets with a CID we lost: try every conn and
            // let quiche accept whichever decrypts it.
            for cidx in self.conns.keys().copied().collect::<Vec<_>>() {
                let info = quiche::RecvInfo {
                    from,
                    to: self.local,
                };
                let ok = if let Some(c) = self.conns.get_mut(&cidx) {
                    c.conn.recv(&mut packet, info).is_ok()
                } else {
                    false
                };
                if ok {
                    self.conn_by_dcid.insert(dcid, cidx);
                    self.process_conn(cidx, now, out);
                    return;
                }
            }
        }
    }

    fn new_conn(
        &mut self,
        dcid: &[u8],
        _scid: &[u8],
        mut packet: Vec<u8>,
        from: SocketAddr,
        now: Instant,
        out: &mut Vec<WtEvent>,
    ) {
        let mut scid = [0u8; 16];
        let mut h = <std::collections::hash_map::RandomState as std::hash::BuildHasher>::build_hasher(&std::collections::hash_map::RandomState::new());
        dcid.hash(&mut h);
        let seed = h.finish();
        for (i, b) in scid.iter_mut().enumerate() {
            *b = seed.rotate_left(i as u32 * 7).to_be_bytes()[i % 8] ^ (i as u64).to_le_bytes()[0] as u8;
        }
        let scid_cid = quiche::ConnectionId::from_ref(&scid);
        let odcid = quiche::ConnectionId::from_ref(dcid);

        let Ok(conn) = quiche::accept(
            &scid_cid,
            Some(&odcid),
            self.local,
            from,
            &mut self.qconfig,
        ) else {
            return;
        };

        let cidx = self.next_cidx;
        self.next_cidx += 1;
        self.conn_by_dcid.insert(dcid.to_vec(), cidx);
        self.conn_by_dcid.insert(scid.to_vec(), cidx);
        self.conns.insert(
            cidx,
            EngineConn {
                conn,
                peer: from,
                control_written: false,
                peer_settings: None,
                streams: HashMap::new(),
                pending: HashMap::new(),
                next_server_uni: 3,
            },
        );

        let info = quiche::RecvInfo {
            from,
            to: self.local,
        };
        if let Some(c) = self.conns.get_mut(&cidx) {
            let _ = c.conn.recv(&mut packet, info);
        }
        self.process_conn(cidx, now, out);
    }

    /// Post-recv: write server control stream once established, then drain
    /// readable streams and datagrams.
    fn process_conn(&mut self, cidx: u64, _now: Instant, out: &mut Vec<WtEvent>) {
        let established = self
            .conns
            .get(&cidx)
            .map(|c| c.conn.is_established())
            .unwrap_or(false);

        if established {
            let c = &mut self.conns.get_mut(&cidx).unwrap();
            if !c.control_written {
                c.control_written = true;
                let mut ctl = Vec::new();
                encode_into(stream_type::CONTROL, &mut ctl);
                let settings = h3::build_frame(frame_type::SETTINGS, &h3::server_settings_payload());
                ctl.extend_from_slice(&settings);
                let sid = c.next_server_uni;
                c.next_server_uni += 4;
                let _ = c.conn.stream_send(sid, &ctl, false);
            }
        }

        // Collect readable stream ids (readable() borrows immutably).
        let readable: Vec<u64> = match self.conns.get(&cidx) {
            Some(c) => c.conn.readable().collect(),
            None => return,
        };

        let mut chunk = vec![0u8; STREAM_CHUNK];
        for sid in readable {
            loop {
                let res = match self.conns.get_mut(&cidx) {
                    Some(c) => c.conn.stream_recv(sid, &mut chunk),
                    None => break,
                };
                match res {
                    Ok((0, true)) => {
                        self.feed_stream(cidx, sid, &[], true, out);
                        break;
                    }
                    Ok((0, false)) => break,
                    Ok((n, fin)) => {
                        let data = chunk[..n].to_vec();
                        self.feed_stream(cidx, sid, &data, fin, out);
                        if fin {
                            break;
                        }
                    }
                    Err(quiche::Error::Done) => break,
                    Err(_) => break,
                }
            }
        }

        // Datagram frames carry WT datagrams (length-prefixed records).
        loop {
            let mut dbuf = vec![0u8; READ_BUF_LEN];
            let res = match self.conns.get_mut(&cidx) {
                Some(c) => c.conn.dgram_recv(&mut dbuf),
                None => break,
            };
            match res {
                Ok(n) => self.feed_quic_dgram(cidx, &dbuf[..n], out),
                Err(_) => break,
            }
        }

        self.flush_pending(cidx);
    }

    fn feed_stream(&mut self, cidx: u64, sid: u64, data: &[u8], fin: bool, out: &mut Vec<WtEvent>) {
        let bidi = sid & 0x03 == 0;
        let entry_exists = self
            .conns
            .get(&cidx)
            .map(|c| c.streams.contains_key(&sid))
            .unwrap_or(false);

        if !entry_exists {
            // New peer stream: start in Classify (uni and bidi prefixes).
            if let Some(c) = self.conns.get_mut(&cidx) {
                c.streams
                    .insert(sid, StreamIn::Classify { buf: Vec::new(), bidi });
            }
        }

        // One immutable probe to learn the current role, then act with
        // mutable borrows only after the probe borrow has ended.
        enum Tag {
            Classify,
            Control,
            Qpack,
            Request { terminal: bool },
            Wt { session: u64 },
            Gone,
        }
        let tag = {
            let Some(c) = self.conns.get(&cidx) else {
                return;
            };
            match c.streams.get(&sid) {
                Some(StreamIn::Classify { .. }) => Tag::Classify,
                Some(StreamIn::Control { .. }) => Tag::Control,
                Some(StreamIn::Qpack) => Tag::Qpack,
                Some(StreamIn::Request { terminal, .. }) => Tag::Request { terminal: *terminal },
                Some(StreamIn::Wt { session }) => Tag::Wt { session: *session },
                None => Tag::Gone,
            }
        };

        match tag {
            Tag::Gone => return,
            Tag::Classify => {
                let buf = {
                    let c = self.conns.get_mut(&cidx).unwrap();
                    match c.streams.get_mut(&sid) {
                        Some(StreamIn::Classify { buf, bidi }) => {
                            buf.extend_from_slice(data);
                            let bidi = *bidi;
                            let snapshot = std::mem::take(buf);
                            (snapshot, bidi)
                        }
                        _ => return,
                    }
                };
                match classify_bytes(&buf.0, buf.1) {
                    None => {
                        // Prefix incomplete: restore buffer, await more.
                        let c = self.conns.get_mut(&cidx).unwrap();
                        if let Some(StreamIn::Classify { buf: slot, .. }) =
                            c.streams.get_mut(&sid)
                        {
                            *slot = buf.0;
                        }
                        if fin {
                            let c = self.conns.get_mut(&cidx).unwrap();
                            c.streams.remove(&sid);
                        }
                    }
                    Some((what, rest)) => match what {
                        Classified::Control => {
                            let c = self.conns.get_mut(&cidx).unwrap();
                            *c.streams.get_mut(&sid).unwrap() = StreamIn::Control { buf: Vec::new() };
                            self.process_control(cidx, sid, &rest, out);
                        }
                        Classified::Qpack => {
                            let c = self.conns.get_mut(&cidx).unwrap();
                            *c.streams.get_mut(&sid).unwrap() = StreamIn::Qpack;
                        }
                        Classified::Request => {
                            let c = self.conns.get_mut(&cidx).unwrap();
                            *c.streams.get_mut(&sid).unwrap() = StreamIn::Request {
                                buf: Vec::new(),
                                request: None,
                                terminal: false,
                            };
                            self.process_request(cidx, sid, &rest, out);
                        }
                        Classified::Wt { session, dir } => {
                            // `session` is the local CONNECT stream id;
                            // translate to the global handle.
                            let handle = match self.handle_by_stream.get(&(cidx, session)) {
                                Some(h) => *h,
                                None => {
                                    let c = self.conns.get_mut(&cidx).unwrap();
                                    c.streams.remove(&sid);
                                    let _ = c
                                        .conn
                                        .stream_shutdown(sid, quiche::Shutdown::Read, 0x100);
                                    return;
                                }
                            };
                            {
                                let c = self.conns.get_mut(&cidx).unwrap();
                                *c.streams.get_mut(&sid).unwrap() = StreamIn::Wt { session: handle };
                            }
                            self.handle_by_stream.insert((cidx, sid), handle);
                            out.push(WtEvent::StreamOpened {
                                session: handle,
                                stream: sid,
                                dir,
                            });
                            if !rest.is_empty() || fin {
                                out.push(WtEvent::StreamData {
                                    session: handle,
                                    stream: sid,
                                    data: rest,
                                    fin,
                                });
                            }
                            if fin {
                                let c = self.conns.get_mut(&cidx).unwrap();
                                c.streams.remove(&sid);
                            }
                        }
                        Classified::Reject => {
                            let c = self.conns.get_mut(&cidx).unwrap();
                            c.streams.remove(&sid);
                            let _ = c
                                .conn
                                .stream_shutdown(sid, quiche::Shutdown::Read, 0x100);
                        }
                    },
                }
            }
            Tag::Control => {
                if data.is_empty() {
                    if fin {
                        let c = self.conns.get_mut(&cidx).unwrap();
                        c.streams.remove(&sid);
                    }
                    return;
                }
                self.process_control(cidx, sid, data, out);
                if fin {
                    let c = self.conns.get_mut(&cidx).unwrap();
                    c.streams.remove(&sid);
                }
            }
            Tag::Qpack => {
                if fin {
                    let c = self.conns.get_mut(&cidx).unwrap();
                    c.streams.remove(&sid);
                }
            }
            Tag::Request { terminal } => {
                if terminal || data.is_empty() {
                    if fin {
                        let c = self.conns.get_mut(&cidx).unwrap();
                        c.streams.remove(&sid);
                    }
                    return;
                }
                self.process_request(cidx, sid, data, out);
                if fin {
                    let c = self.conns.get_mut(&cidx).unwrap();
                    c.streams.remove(&sid);
                }
            }
            Tag::Wt { session } => {
                out.push(WtEvent::StreamData {
                    session,
                    stream: sid,
                    data: data.to_vec(),
                    fin,
                });
                if fin {
                    let c = self.conns.get_mut(&cidx).unwrap();
                    c.streams.remove(&sid);
                }
            }
        }
    }
}

/// Try to turn a `Classify` buffer into a concrete role.
/// Returns `(role, post-prefix bytes)` or None when the prefix varint(s)
/// are still incomplete.
fn classify_bytes(buf: &[u8], bidi: bool) -> Option<(Classified, Vec<u8>)> {
    let (prefix, n) = varint::decode(buf)?;
    if bidi {
        if prefix == stream_type::WT_BIDI {
            let (qsid, n2) = varint::decode(&buf[n..])?;
            Some((
                Classified::Wt {
                    session: qsid,
                    dir: StreamDir::Bidi,
                },
                buf[n + n2..].to_vec(),
            ))
        } else if prefix == frame_type::HEADERS || prefix == frame_type::DATA {
            // h3 frame: the varint we peeked *is* the frame type, keep all.
            Some((Classified::Request, buf.to_vec()))
        } else {
            Some((Classified::Reject, Vec::new()))
        }
    } else {
        match prefix {
            stream_type::CONTROL => Some((Classified::Control, buf[n..].to_vec())),
            stream_type::WT_UNI_CLIENT => {
                let (qsid, n2) = varint::decode(&buf[n..])?;
                Some((
                    Classified::Wt {
                        session: qsid,
                        dir: StreamDir::UniClient,
                    },
                    buf[n + n2..].to_vec(),
                ))
            }
            stream_type::QPACK_ENCODER | stream_type::QPACK_DECODER => {
                Some((Classified::Qpack, Vec::new()))
            }
            _ => Some((Classified::Reject, Vec::new())),
        }
    }
}

impl WebTransportServer {
    fn process_control(&mut self, cidx: u64, sid: u64, data: &[u8], _out: &mut Vec<WtEvent>) {
        let mut acc = data.to_vec();
        loop {
            let (kind, payload, consumed) = match h3::try_parse_frame(&acc) {
                Ok(v) => v,
                Err(_) => break,
            };
            acc.drain(..consumed);
            if kind == frame_type::SETTINGS {
                let ps = h3::parse_settings(&payload);
                if let Some(c) = self.conns.get_mut(&cidx) {
                    c.peer_settings = Some(ps);
                }
            }
        }
        if let Some(c) = self.conns.get_mut(&cidx) {
            if let Some(StreamIn::Control { buf }) = c.streams.get_mut(&sid) {
                buf.extend_from_slice(&acc);
                if buf.len() > MAX_HEADER_BLOCK {
                    c.streams.insert(sid, StreamIn::Qpack);
                }
            }
        }
    }

    fn process_request(&mut self, cidx: u64, sid: u64, data: &[u8], out: &mut Vec<WtEvent>) {
        let mut acc = data.to_vec();
        loop {
            let (kind, payload, consumed) = match h3::try_parse_frame(&acc) {
                Ok(v) => v,
                Err(_) => break,
            };
            acc.drain(..consumed);

            let terminal = self
                .conns
                .get(&cidx)
                .and_then(|c| c.streams.get(&sid))
                .map(|s| matches!(s, StreamIn::Request { terminal: true, .. }))
                .unwrap_or(false);
            if terminal {
                continue;
            }

            match kind {
                frame_type::HEADERS => {
                    let request = match Self::decode_headers(&payload) {
                        Some(r) => r,
                        None => {
                            self.finish_request(cidx, sid, 400);
                            continue;
                        }
                    };
                    if request.is_webtransport_connect() {
                        let handle = self.next_handle;
                        self.next_handle += 1;
                        self.sessions.insert(
                            handle,
                            SessionRec {
                                cidx,
                                request_stream: sid,
                                request: request.clone(),
                                accepted: false,
                                closed: false,
                            },
                        );
                        self.handle_by_stream.insert((cidx, sid), handle);
                        if let Some(c) = self.conns.get_mut(&cidx) {
                            if let Some(StreamIn::Request {
                                request: slot, ..
                            }) = c.streams.get_mut(&sid)
                            {
                                *slot = Some(request.clone());
                            }
                            // Any WT stream already tagged with this session
                            // id (races with CONNECT read order) can be moved
                            // into Wt state.
                            let qkey = (cidx, sid);
                            let _ = qkey;
                        }
                        out.push(WtEvent::SessionOpened {
                            session: handle,
                            request,
                        });
                    } else if request.method() == "GET" {
                        // Browser origin probe: advertise draft support.
                        let resp = WtResponse::http3_probe();
                        self.send_response(cidx, sid, &resp, true);
                        self.finish_request(cidx, sid, 0);
                    } else {
                        let resp = WtResponse::status(405);
                        self.send_response(cidx, sid, &resp, true);
                        self.finish_request(cidx, sid, 405);
                    }
                }
                frame_type::DATA => {
                    // Capsule space of an established session.
                    let mut cur: &[u8] = &payload;
                    // DATA frames here are length-prefixed capsules; the
                    // frame payload may hold several.
                    while let Some((ctype, value, used)) = h3::try_parse_capsule(cur) {
                        cur = &cur[used..];
                        if ctype == h3::capsule_type::CLOSE_SESSION {
                            let (code, reason) =
                                h3::parse_close_capsule_value(&value).unwrap_or((0, String::new()));
                            self.close_session_peer(cidx, sid, code, reason, out);
                        }
                    }
                }
                _ => {}
            }
        }
        if let Some(c) = self.conns.get_mut(&cidx) {
            if let Some(StreamIn::Request { buf, .. }) = c.streams.get_mut(&sid) {
                buf.extend_from_slice(&acc);
                if buf.len() > MAX_HEADER_BLOCK {
                    self.finish_request(cidx, sid, 431);
                }
            }
        }
    }

    fn decode_headers(payload: &[u8]) -> Option<WtRequest> {
        let mut cur = Bytes::copy_from_slice(payload);
        let decoded = decode_stateless(&mut cur, MAX_HEADER_BLOCK as u64).ok()?;
        if decoded.dyn_ref {
            return None; // we advertise capacity 0; reject dynamic refs
        }
        WtRequest::from_fields(&decoded.fields).ok()
    }

    fn finish_request(&mut self, cidx: u64, sid: u64, _status: u16) {
        if let Some(c) = self.conns.get_mut(&cidx) {
            if let Some(StreamIn::Request { terminal, .. }) = c.streams.get_mut(&sid) {
                *terminal = true;
            }
        }
    }

    fn close_session_peer(
        &mut self,
        cidx: u64,
        sid: u64,
        code: u64,
        reason: String,
        out: &mut Vec<WtEvent>,
    ) {
        let found = self
            .sessions
            .iter()
            .find(|(_, s)| s.cidx == cidx && s.request_stream == sid)
            .map(|(h, _)| *h);
        if let Some(handle) = found {
            if let Some(s) = self.sessions.get_mut(&handle) {
                if s.closed {
                    return;
                }
                s.closed = true;
            }
            out.push(WtEvent::SessionClosed {
                session: handle,
                code,
                reason,
            });
            self.drop_session_maps(handle, cidx, sid);
            self.sessions.remove(&handle);
        }
    }

    fn drop_session_maps(&mut self, handle: u64, cidx: u64, request_stream: u64) {
        self.handle_by_stream.remove(&(cidx, request_stream));
        // Remove WT stream maps of this session too.
        let doomed: Vec<_> = self
            .handle_by_stream
            .iter()
            .filter(|(_, h)| **h == handle)
            .map(|(k, _)| *k)
            .collect();
        for k in doomed {
            self.handle_by_stream.remove(&k);
        }
        if let Some(c) = self.conns.get_mut(&cidx) {
            let doomed: Vec<u64> = c
                .streams
                .iter()
                .filter(|(_, s)| matches!(s, StreamIn::Wt { session } if *session == handle))
                .map(|(id, _)| *id)
                .collect();
            for id in doomed {
                c.streams.remove(&id);
            }
        }
    }

    // ---------------- application API ----------------

    /// Accept a proposed session: replies `:status: 200` (echoing the client
    /// session-id tag when present) and marks it usable.
    pub fn accept_session(&mut self, session: u64) -> io::Result<()> {
        let s = self
            .sessions
            .get(&session)
            .ok_or_else(|| io::Error::other("no such session"))?;
        if s.closed {
            return Err(io::Error::other("session closed"));
        }
        let mut resp = WtResponse::ok();
        if let Some(tag) = s.request.session_id_tag() {
            resp = resp.header(WT_SESSION_ID_HEADER, tag);
        }
        let (cidx, sid) = (s.cidx, s.request_stream);
        let _ = self.conns.get_mut(&cidx).ok_or_else(|| io::Error::other("conn gone"))?;
        self.send_response(cidx, sid, &resp, false);
        if let Some(s) = self.sessions.get_mut(&session) {
            s.accepted = true;
        }
        // Register this request stream's data as capsules space (already is)
        // and promote any pending classification done.
        Ok(())
    }

    /// Reject a session proposal with an HTTP status.
    pub fn reject_session(&mut self, session: u64, status: u16) -> io::Result<()> {
        let s = self
            .sessions
            .get(&session)
            .ok_or_else(|| io::Error::other("no such session"))?;
        let (cidx, sid) = (s.cidx, s.request_stream);
        self.send_response(cidx, sid, &WtResponse::status(status), true);
        self.finish_request(cidx, sid, status);
        if let Some(s) = self.sessions.get_mut(&session) {
            s.closed = true;
        }
        self.drop_session_maps(session, cidx, sid);
        self.sessions.remove(&session);
        Ok(())
    }

    fn send_response(&mut self, cidx: u64, sid: u64, resp: &WtResponse, fin: bool) {
        let mut block = BytesMut::new();
        if encode_stateless(&mut block, resp.to_fields().iter()).is_err() {
            return;
        }
        let frame = h3::build_frame(frame_type::HEADERS, &block);
        self.write_stream(cidx, sid, frame, fin);
    }

    /// Send bytes on a WT stream of the session (`stream` is the id from
    /// `StreamOpened`).
    pub fn stream_send(&mut self, session: u64, stream: u64, data: &[u8], fin: bool) -> io::Result<()> {
        let s = self
            .sessions
            .get(&session)
            .ok_or_else(|| io::Error::other("no such session"))?;
        if s.closed {
            return Err(io::Error::other("session closed"));
        }
        let cidx = s.cidx;
        self.handle_by_stream.insert((cidx, stream), session);
        self.write_stream(cidx, stream, data.to_vec(), fin);
        Ok(())
    }

    /// Half/fully close a WT stream.
    pub fn stream_close(&mut self, session: u64, stream: u64) -> io::Result<()> {
        let s = self.sessions.get(&session).ok_or_else(|| io::Error::other("no such session"))?;
        let cidx = s.cidx;
        self.write_stream(cidx, stream, Vec::new(), true);
        Ok(())
    }

    /// Abort a WT stream with the WebTransport application error code.
    pub fn stream_reset(&mut self, session: u64, stream: u64, error: u64) -> io::Result<()> {
        let s = self.sessions.get(&session).ok_or_else(|| io::Error::other("no such session"))?;
        let cidx = s.cidx;
        if let Some(c) = self.conns.get_mut(&cidx) {
            let _ = c.conn.stream_shutdown(stream, quiche::Shutdown::Write, error);
        }
        Ok(())
    }

    /// Server-initiated unidirectional WT stream inside a session; returns
    /// its stream id.
    pub fn open_uni_stream(&mut self, session: u64) -> io::Result<u64> {
        let s = self
            .sessions
            .get(&session)
            .ok_or_else(|| io::Error::other("no such session"))?;
        if s.closed {
            return Err(io::Error::other("session closed"));
        }
        let cidx = s.cidx;
        let local_session = s.request_stream;
        let c = self.conns.get_mut(&cidx).ok_or_else(|| io::Error::other("conn gone"))?;
        let sid = c.next_server_uni;
        c.next_server_uni += 4;
        let mut prefix = Vec::new();
        encode_into(stream_type::WT_UNI_SERVER, &mut prefix);
        encode_into(local_session, &mut prefix);
        let _ = c.conn.stream_send(sid, &prefix, false);
        self.handle_by_stream.insert((cidx, sid), session);
        Ok(sid)
    }

    /// Send a WT datagram (length-prefixed record framing) to the session.
    pub fn send_datagram(&mut self, session: u64, data: &[u8]) -> io::Result<()> {
        let s = self
            .sessions
            .get(&session)
            .ok_or_else(|| io::Error::other("no such session"))?;
        let cidx = s.cidx;
        let local_session = s.request_stream;
        let mut content = Vec::with_capacity(8 + data.len());
        encode_into(local_session, &mut content);
        content.extend_from_slice(data);
        let mut rec = Vec::with_capacity(8 + content.len());
        encode_into(content.len() as u64, &mut rec);
        rec.extend_from_slice(&content);
        let c = self.conns.get_mut(&cidx).ok_or_else(|| io::Error::other("conn gone"))?;
        c.conn
            .dgram_send(&rec)
            .map_err(|e| io::Error::other(format!("dgram_send: {e}")))
    }

    /// Gracefully close a session: sends the CLOSE_WEBTRANSPORT_SESSION
    /// capsule on the request stream, resets WT streams and emits
    /// `SessionClosed`.
    pub fn close_session(&mut self, session: u64, code: u64, reason: &str) -> io::Result<()> {
        let s = self
            .sessions
            .get(&session)
            .ok_or_else(|| io::Error::other("no such session"))?;
        if s.closed {
            return Ok(());
        }
        let (cidx, sid) = (s.cidx, s.request_stream);
        let capsule = h3::build_close_capsule(code, reason);
        let frame = h3::frame_capsule(&capsule);
        self.write_stream(cidx, sid, frame, false);
        // Reset active WT streams of this session.
        if let Some(c) = self.conns.get_mut(&cidx) {
            let ids: Vec<u64> = c
                .streams
                .iter()
                .filter(|(_, st)| matches!(st, StreamIn::Wt { session: h } if *h == session))
                .map(|(id, _)| *id)
                .collect();
            for id in ids {
                let _ = c
                    .conn
                    .stream_shutdown(id, quiche::Shutdown::Write, h3::WT_APPLICATION_ERROR);
                c.streams.remove(&id);
            }
        }
        if let Some(s) = self.sessions.get_mut(&session) {
            s.closed = true;
        }
        self.drop_session_maps(session, cidx, sid);
        Ok(())
    }

    /// Close the whole connection.
    pub fn close(&mut self) -> io::Result<()> {
        for (cidx, c) in self.conns.iter_mut() {
            let _ = c
                .conn
                .close(true, 0, b"server closing");
            let _ = cidx;
        }
        Ok(())
    }

    // ---------------- write plumbing ----------------

    fn write_stream(&mut self, cidx: u64, sid: u64, data: Vec<u8>, fin: bool) {
        let Some(c) = self.conns.get_mut(&cidx) else {
            return;
        };
        let mut off = 0usize;
        loop {
            let want_fin = fin && off == data.len();
            match c.conn.stream_send(sid, &data[off..], want_fin) {
                Ok(n) => {
                    off += n;
                    if off >= data.len() {
                        if !fin {
                            return;
                        }
                        if want_fin {
                            return;
                        }
                        // fin still owed but data exhausted: next call sends
                        // empty slice with fin unless blocked.
                        match c.conn.stream_send(sid, &[], true) {
                            Ok(_) => return,
                            Err(_) => {
                                c.pending
                                    .entry(sid)
                                    .or_default()
                                    .push_back(WriteChunk { data: Vec::new(), pos: 0, fin: true });
                                return;
                            }
                        }
                    }
                }
                Err(_) => {
                    c.pending.entry(sid).or_default().push_back(WriteChunk {
                        data: data[off..].to_vec(),
                        pos: 0,
                        fin,
                    });
                    return;
                }
            }
        }
    }

    fn flush_pending(&mut self, cidx: u64) {
        let Some(c) = self.conns.get_mut(&cidx) else {
            return;
        };
        let sids: Vec<u64> = c.pending.keys().copied().collect();
        for sid in sids {
            loop {
                let head = match c.pending.get_mut(&sid).and_then(|q| q.front_mut()) {
                    Some(h) => h,
                    None => break,
                };
                match c.conn.stream_send(sid, &head.data[head.pos..], head.fin) {
                    Ok(n) => {
                        head.pos += n;
                        if head.pos >= head.data.len() {
                            c.pending.get_mut(&sid).unwrap().pop_front();
                        } else {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
            if c.pending.get(&sid).map(|q| q.is_empty()).unwrap_or(false) {
                c.pending.remove(&sid);
            }
        }
    }

    fn flush_send(&mut self, cidx: u64, out: &mut Vec<WtEvent>) {        let mut buf = vec![0u8; READ_BUF_LEN];
        loop {
            let res = match self.conns.get_mut(&cidx) {
                Some(c) => c.conn.send(&mut buf),
                None => return,
            };
            match res {
                Ok((n, info)) => {
                    let _ = self.socket.send_to(&buf[..n], info.to);
                }
                Err(quiche::Error::Done) => return,
                Err(_) => {
                    self.remove_conn(cidx, "send error", out);
                    return;
                }
            }
        }
    }

    fn feed_quic_dgram(&mut self, cidx: u64, packet: &[u8], out: &mut Vec<WtEvent>) {
        // Length-prefixed records: varint(len) [ varint(session) payload ].
        let mut cur = packet;
        while !cur.is_empty() {
            let (rlen, n1) = match varint::decode(cur) {
                Some(v) => v,
                None => return,
            };
            let body = match cur.get(n1..n1 + rlen as usize) {
                Some(b) => b,
                None => return,
            };
            cur = &cur[n1 + rlen as usize..];
            let (qsid, n2) = match varint::decode(body) {
                Some(v) => v,
                None => return,
            };
            if let Some(&handle) = self.handle_by_stream.get(&(cidx, qsid)) {
                out.push(WtEvent::Datagram {
                    session: handle,
                    data: body[n2..].to_vec(),
                });
            }
        }
    }

    fn reap_closed(&mut self, out: &mut Vec<WtEvent>) {
        let doomed: Vec<u64> = self
            .conns
            .iter()
            .filter(|(_, c)| c.conn.is_closed())
            .map(|(k, _)| *k)
            .collect();
        for cidx in doomed {
            let reason = self
                .conns
                .get(&cidx)
                .and_then(|c| c.conn.peer_error())
                .map(|e| format!("peer error: {e:?}"))
                .unwrap_or_else(|| "connection closed".into());
            self.remove_conn(cidx, &reason, out);
        }
    }

    fn remove_conn(&mut self, cidx: u64, reason: &str, out: &mut Vec<WtEvent>) {
        let handles: Vec<u64> = self
            .sessions
            .iter()
            .filter(|(_, s)| s.cidx == cidx && !s.closed)
            .map(|(h, _)| *h)
            .collect();
        for h in handles {
            out.push(WtEvent::SessionClosed {
                session: h,
                code: h3::WT_APPLICATION_ERROR,
                reason: reason.to_string(),
            });
            self.sessions.remove(&h);
        }
        self.conns.remove(&cidx);
        self.conn_by_dcid.retain(|_, v| *v != cidx);
        self.handle_by_stream.retain(|key, _| key.0 != cidx);
        out.push(WtEvent::ConnectionClosed {
            reason: reason.to_string(),
        });
    }
}

enum Classified {
    Control,
    Qpack,
    Request,
    Wt { session: u64, dir: StreamDir },
    Reject,
}

impl Drop for WebTransportServer {
    fn drop(&mut self) {
        // Dropping socket sources deregisters from epoll automatically.
    }
}
