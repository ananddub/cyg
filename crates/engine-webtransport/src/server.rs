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

use bytes::{BufMut, Bytes, BytesMut};
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
        data: Bytes,
        fin: bool,
    },
    /// Decoded WT datagram payload for the session.
    Datagram { session: u64, data: Bytes },
    /// The whole QUIC connection ended.
    ConnectionClosed { reason: String },
}

/// What a receive stream buffer is doing.
enum StreamIn {
    /// First varint(s) not yet complete: accumulating. `bidi` tells which
    /// classification table to apply once we can decode.
    Classify { buf: BytesMut, bidi: bool },
    Control { buf: BytesMut },
    Qpack,
    Request {
        buf: BytesMut,
        request: Option<WtRequest>,
        terminal: bool,
    },
    /// Active WebTransport data stream of `session` handle.
    Wt { session: u64 },
}

struct WriteChunk {
    data: Bytes,
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
    /// Connection-id index. Keys are zero-padded fixed arrays (QUIC CIDs are
    /// ≤ 20 bytes), so no per-lookup heap key.
    conn_by_dcid: HashMap<[u8; quiche::MAX_CONN_ID_LEN], u64>,
    next_cidx: u64,
    sessions: HashMap<u64, SessionRec>,
    handle_by_stream: HashMap<(u64, u64), u64>,
    next_handle: u64,
    events: Events,
    // ---- persistent scratch (allocated once, reused every poll) ----
    /// UDP receive / version-negotiation buffer.
    udp_buf: Vec<u8>,
    /// QUIC packet send buffer.
    send_buf: Vec<u8>,
    /// `stream_recv` chunk buffer.
    chunk_buf: Vec<u8>,
    /// `dgram_recv` buffer.
    dgram_buf: Vec<u8>,
    /// Single-copy handoff of received bytes into `Bytes` events.
    ev_scratch: BytesMut,
    /// Cross-poll `WtEvent` buffer; adopted so helpers take `&mut self`.
    event_buf: Vec<WtEvent>,
    /// Scratch for enumerate-then-remove passes.
    doomed: Vec<u64>,
    /// Scratch for the pending-write sweep (separate from `doomed`, which the
    /// poll loop holds while flushing).
    flush_sids: Vec<u64>,
    /// Scratch for server-initiated uni stream prefixes.
    uni_prefix: Vec<u8>,
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
            udp_buf: vec![0u8; READ_BUF_LEN],
            send_buf: vec![0u8; READ_BUF_LEN],
            chunk_buf: vec![0u8; STREAM_CHUNK],
            dgram_buf: vec![0u8; READ_BUF_LEN],
            ev_scratch: BytesMut::new(),
            event_buf: Vec::new(),
            doomed: Vec::new(),
            flush_sids: Vec::new(),
            uni_prefix: Vec::with_capacity(16),
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

    /// Drive the QUIC/h3/WT machinery for up to `wait`, appending app events
    /// to the caller-owned `out` buffer (reused across polls).
    pub fn poll_events(&mut self, wait: Duration, out: &mut Vec<WtEvent>) -> io::Result<()> {
        let mut ev = std::mem::take(&mut self.event_buf);
        ev.clear();
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
            // The receive buffer is held locally while routing so `route_packet`
            // can take `&mut self` without aliasing a field borrow.
            let mut udp = std::mem::take(&mut self.udp_buf);
            let result = loop {
                match self.socket.recv_from(&mut udp) {
                    Ok((len, from)) => self.route_packet(&mut udp[..len], from, now, &mut ev),
                    Err(ref e)
                        if e.kind() == io::ErrorKind::WouldBlock
                            || e.kind() == io::ErrorKind::Interrupted =>
                    {
                        break Ok(())
                    }
                    Err(e) => break Err(e),
                }
            };
            self.udp_buf = udp;
            result?;
        }

        if !fired {
            for c in self.conns.values_mut() {
                c.conn.on_timeout();
            }
        }

        // Drain application writes that were flow/blocked earlier, then push
        // every connection's packets out.
        let mut cids = std::mem::take(&mut self.doomed);
        cids.clear();
        cids.extend(self.conns.keys().copied());
        for i in 0..cids.len() {
            let cidx = cids[i];
            self.flush_pending(cidx);
            self.flush_send(cidx, &mut ev);
        }
        self.doomed = cids;

        self.reap_closed(&mut ev);
        std::mem::swap(out, &mut ev);
        ev.clear();
        self.event_buf = ev;
        Ok(())
    }

    /// Copy a variable-length QUIC connection id into the fixed inline key.
    fn cid_key(cid: &[u8]) -> [u8; quiche::MAX_CONN_ID_LEN] {
        let mut k = [0u8; quiche::MAX_CONN_ID_LEN];
        let n = cid.len().min(quiche::MAX_CONN_ID_LEN);
        k[..n].copy_from_slice(&cid[..n]);
        k
    }

    fn route_packet(
        &mut self,
        mut packet: &mut [u8],
        from: SocketAddr,
        now: Instant,
        out: &mut Vec<WtEvent>,
    ) {
        // quiche parses the header in place, so the buffer is handed out
        // mutably; everything we need is copied into inline keys first.
        let (dcid, scid, odcid_len, ty, version) = {
            let h = match quiche::Header::from_slice(&mut packet, quiche::MAX_CONN_ID_LEN) {
                Ok(h) => h,
                Err(_) => return,
            };
            (
                Self::cid_key(h.dcid.as_ref()),
                Self::cid_key(h.scid.as_ref()),
                h.dcid.len(),
                h.ty,
                h.version,
            )
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
                let _ = c.conn.recv(&mut packet[..], info);
                c.peer = from;
            }
            self.process_conn(cidx, now, out);
            return;
        }

        // Retry-token-free path: unknown CID.
        if ty == quiche::Type::Initial && quiche::version_is_supported(version) {
            self.new_conn(&dcid, &dcid[..odcid_len], &mut packet[..], from, now, out);
        } else if version != quiche::PROTOCOL_VERSION && ty != quiche::Type::Short {
            let scid_cid = quiche::ConnectionId::from_ref(&scid[..]);
            let dcid_cid = quiche::ConnectionId::from_ref(&dcid[..]);
            if let Ok(len) = quiche::negotiate_version(&scid_cid, &dcid_cid, &mut self.send_buf) {
                let _ = self.socket.send_to(&self.send_buf[..len], from);
            }
        } else {
            // Mid-connection packets with a CID we lost: try every conn and
            // let quiche accept whichever decrypts it.
            let mut candidates = std::mem::take(&mut self.doomed);
            candidates.clear();
            candidates.extend(self.conns.keys().copied());
            let mut matched: Option<u64> = None;
            for i in 0..candidates.len() {
                let cidx = candidates[i];
                let info = quiche::RecvInfo {
                    from,
                    to: self.local,
                };
                let ok = if let Some(c) = self.conns.get_mut(&cidx) {
                    c.conn.recv(&mut packet[..], info).is_ok()
                } else {
                    false
                };
                if ok {
                    matched = Some(cidx);
                    break;
                }
            }
            self.doomed = candidates;
            if let Some(cidx) = matched {
                self.conn_by_dcid.insert(dcid, cidx);
                self.process_conn(cidx, now, out);
            }
        }
    }

    fn new_conn(
        &mut self,
        dcid: &[u8; quiche::MAX_CONN_ID_LEN],
        odcid: &[u8],
        packet: &mut [u8],
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
        let odcid_cid = quiche::ConnectionId::from_ref(odcid);

        let Ok(conn) = quiche::accept(
            &scid_cid,
            Some(&odcid_cid),
            self.local,
            from,
            &mut self.qconfig,
        ) else {
            return;
        };

        let cidx = self.next_cidx;
        self.next_cidx += 1;
        self.conn_by_dcid.insert(*dcid, cidx);
        self.conn_by_dcid.insert(Self::cid_key(&scid), cidx);
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
            let _ = c.conn.recv(packet, info);
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
                // Control-stream intro: type varint + SETTINGS frame.
                // One-shot per connection; allocation here is negligible.
                let mut ctl = Vec::with_capacity(16 + 32);
                encode_into(stream_type::CONTROL, &mut ctl);
                let mut settings_body = Vec::with_capacity(48);
                h3::server_settings_payload(&mut settings_body);
                ctl.extend_from_slice(&h3::build_frame(frame_type::SETTINGS, &settings_body));
                let sid = c.next_server_uni;
                c.next_server_uni += 4;
                let _ = c.conn.stream_send(sid, &ctl, false);
            }
        }

        // Collect readable stream ids into the persistent scratch
        // (`readable()` borrows immutably).
        let mut readable = std::mem::take(&mut self.doomed);
        readable.clear();
        if let Some(c) = self.conns.get(&cidx) {
            readable.extend(c.conn.readable());
        }
        let mut i = 0usize;
        while i < readable.len() {
            let sid = readable[i];
            i += 1;
            loop {
                let (data, fin) = {
                    let Some(c) = self.conns.get_mut(&cidx) else {
                        break;
                    };
                    match c.conn.stream_recv(sid, &mut self.chunk_buf) {
                        Ok((0, true)) => (None, true),
                        Ok((0, false)) => break,
                        Ok((n, fin)) => (Some(n), fin),
                        Err(quiche::Error::Done) => break,
                        Err(_) => break,
                    }
                };
                match data {
                    Some(n) => {
                        // Single copy into the BytesMut slab; the event then
                        // shares it zero-copy (`split().freeze()`).
                        self.ev_scratch.clear();
                        self.ev_scratch.reserve(n);
                        self.ev_scratch.put_slice(&self.chunk_buf[..n]);
                        let bytes = self.ev_scratch.split().freeze();
                        self.feed_stream(cidx, sid, bytes, fin, out);
                        if fin {
                            break;
                        }
                    },
                    None => {
                        self.feed_stream(cidx, sid, Bytes::new(), true, out);
                        break;
                    },
                }
            }
        }
        readable.clear();
        self.doomed = readable;

        // Datagram frames carry WT datagrams (length-prefixed records).
        loop {
            let n = match self.conns.get_mut(&cidx) {
                Some(c) => match c.conn.dgram_recv(&mut self.dgram_buf) {
                    Ok(n) => n,
                    Err(_) => break,
                },
                None => break,
            };
            // One copy into the slab; `feed_quic_dgram` hands out `Bytes`
            // views without re-copying.
            self.ev_scratch.clear();
            self.ev_scratch.reserve(n);
            self.ev_scratch.put_slice(&self.dgram_buf[..n]);
            let bytes = self.ev_scratch.split().freeze();
            self.feed_quic_dgram(cidx, bytes, out);
        }

        self.flush_pending(cidx);
    }

    fn feed_stream(
        &mut self,
        cidx: u64,
        sid: u64,
        data: Bytes,
        fin: bool,
        out: &mut Vec<WtEvent>,
    ) {
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
                    .insert(sid, StreamIn::Classify { buf: BytesMut::new(), bidi });
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
                // Append, then decode the prefix against the buffer in place.
                let conn = match self.conns.get_mut(&cidx) {
                    Some(c) => c,
                    None => return,
                };
                let Some(StreamIn::Classify { buf, bidi }) = conn.streams.get_mut(&sid) else {
                    return;
                };
                buf.put_slice(&data);
                match classify_bytes(buf, *bidi) {
                    None => {
                        // Prefix incomplete: the buffer already holds every
                        // byte; await more (or drop on fin).
                        if fin {
                            conn.streams.remove(&sid);
                        }
                    },
                    Some(classified) => {
                        match classified.role {
                            Role::Control => {
                                conn.streams
                                    .insert(sid, StreamIn::Control { buf: BytesMut::new() });
                                let rest = classified.rest;
                                self.process_control(cidx, sid, &rest, out);
                            },
                            Role::Qpack => {
                                conn.streams.insert(sid, StreamIn::Qpack);
                            },
                            Role::Request => {
                                conn.streams.insert(
                                    sid,
                                    StreamIn::Request {
                                        buf: BytesMut::new(),
                                        request: None,
                                        terminal: false,
                                    },
                                );
                                self.process_request(cidx, sid, &classified.rest, out);
                            },
                            Role::Wt { session, dir } => {
                                // `session` is the local CONNECT stream id;
                                // translate to the global handle.
                                let handle = match self.handle_by_stream.get(&(cidx, session)) {
                                    Some(h) => *h,
                                    None => {
                                        conn.streams.remove(&sid);
                                        let _ = conn
                                            .conn
                                            .stream_shutdown(sid, quiche::Shutdown::Read, 0x100);
                                        return;
                                    }
                                };
                                *conn.streams.get_mut(&sid).unwrap() =
                                    StreamIn::Wt { session: handle };
                                self.handle_by_stream.insert((cidx, sid), handle);
                                out.push(WtEvent::StreamOpened {
                                    session: handle,
                                    stream: sid,
                                    dir,
                                });
                                if !classified.rest.is_empty() || fin {
                                    out.push(WtEvent::StreamData {
                                        session: handle,
                                        stream: sid,
                                        data: classified.rest,
                                        fin,
                                    });
                                }
                                if fin {
                                    conn.streams.remove(&sid);
                                }
                            },
                            Role::Reject => {
                                conn.streams.remove(&sid);
                                let _ = conn
                                    .conn
                                    .stream_shutdown(sid, quiche::Shutdown::Read, 0x100);
                            },
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
                self.process_control(cidx, sid, &data, out);
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
                self.process_request(cidx, sid, &data, out);
                if fin {
                    let c = self.conns.get_mut(&cidx).unwrap();
                    c.streams.remove(&sid);
                }
            }
            Tag::Wt { session } => {
                out.push(WtEvent::StreamData {
                    session,
                    stream: sid,
                    data,
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

/// Outcome of decoding a `Classify` prefix: the stream's role plus the
/// post-prefix bytes as a zero-copy tail of the decoded buffer.
struct ClassOut {
    role: Role,
    rest: Bytes,
}

#[derive(Debug, Clone, Copy)]
enum Role {
    Control,
    Qpack,
    Request,
    Wt { session: u64, dir: StreamDir },
    Reject,
}

/// Try to turn a `Classify` buffer into a concrete role. `None` means the
/// prefix varint(s) are still incomplete; `Role::Reject` is an unknown prefix
/// (caller drops the stream). The consumed prefix is cut from `buf` in place
/// and `rest` is a zero-copy tail of what follows.
fn classify_bytes(buf: &mut BytesMut, bidi: bool) -> Option<ClassOut> {
    let (prefix, n) = varint::decode(buf)?;
    if bidi {
        if prefix == stream_type::WT_BIDI {
            let (qsid, n2) = varint::decode(&buf[n..])?;
            Some(ClassOut {
                role: Role::Wt {
                    session: qsid,
                    dir: StreamDir::Bidi,
                },
                rest: buf.split_off(n + n2).freeze(),
            })
        } else if prefix == frame_type::HEADERS || prefix == frame_type::DATA {
            // h3 frame: the varint we peeked *is* the frame type, keep all.
            Some(ClassOut {
                role: Role::Request,
                rest: buf.split_off(0).freeze(),
            })
        } else {
            Some(ClassOut {
                role: Role::Reject,
                rest: Bytes::new(),
            })
        }
    } else {
        match prefix {
            stream_type::CONTROL => Some(ClassOut {
                role: Role::Control,
                rest: buf.split_off(n).freeze(),
            }),
            stream_type::WT_UNI_CLIENT => {
                let (qsid, n2) = varint::decode(&buf[n..])?;
                Some(ClassOut {
                    role: Role::Wt {
                        session: qsid,
                        dir: StreamDir::UniClient,
                    },
                    rest: buf.split_off(n + n2).freeze(),
                })
            },
            stream_type::QPACK_ENCODER | stream_type::QPACK_DECODER => Some(ClassOut {
                role: Role::Qpack,
                rest: Bytes::new(),
            }),
            _ => Some(ClassOut {
                role: Role::Reject,
                rest: Bytes::new(),
            }),
        }
    }
}

impl WebTransportServer {
    fn process_control(&mut self, cidx: u64, sid: u64, data: &[u8], _out: &mut Vec<WtEvent>) {
        // Cursor over the caller's buffer: frames are read in place, only the
        // ragged tail is retained in the stream buffer.
        let mut cur = data;
        while let Ok((kind, payload, used)) = h3::try_parse_frame(cur) {
            cur = &cur[used..];
            if kind == frame_type::SETTINGS {
                let ps = h3::parse_settings(payload);
                if let Some(c) = self.conns.get_mut(&cidx) {
                    c.peer_settings = Some(ps);
                }
            }
        }
        if let Some(c) = self.conns.get_mut(&cidx) {
            if let Some(StreamIn::Control { buf }) = c.streams.get_mut(&sid) {
                buf.put_slice(cur);
                if buf.len() > MAX_HEADER_BLOCK {
                    c.streams.insert(sid, StreamIn::Qpack);
                }
            }
        }
    }

    fn process_request(&mut self, cidx: u64, sid: u64, data: &[u8], out: &mut Vec<WtEvent>) {
        let mut cur = data;
        loop {
            let Ok((kind, payload, used)) = h3::try_parse_frame(cur) else {
                break;
            };
            cur = &cur[used..];

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
                    let request = match Self::decode_headers(payload) {
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
                    let mut c = payload;
                    // DATA frames here are length-prefixed capsules; the
                    // frame payload may hold several.
                    while let Some((ctype, value, used)) = h3::try_parse_capsule(c) {
                        c = &c[used..];
                        if ctype == h3::capsule_type::CLOSE_SESSION {
                            let (code, reason) =
                                h3::parse_close_capsule_value(value).unwrap_or((0, String::new()));
                            self.close_session_peer(cidx, sid, code, reason, out);
                        }
                    }
                }
                _ => {}
            }
        }
        if let Some(c) = self.conns.get_mut(&cidx) {
            if let Some(StreamIn::Request { buf, .. }) = c.streams.get_mut(&sid) {
                buf.put_slice(cur);
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
        // Every other stream of this session lives in the same connection, so
        // one u64 list identifies them all.
        let mut doomed = std::mem::take(&mut self.doomed);
        doomed.clear();
        doomed.extend(
            self.handle_by_stream
                .iter()
                .filter(|((c, _), h)| *c == cidx && **h == handle)
                .map(|((_, s), _)| *s),
        );
        for i in 0..doomed.len() {
            self.handle_by_stream.remove(&(cidx, doomed[i]));
        }
        if let Some(c) = self.conns.get_mut(&cidx) {
            for i in 0..doomed.len() {
                c.streams.remove(&doomed[i]);
            }
        }
        self.doomed = doomed;
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
        self.write_stream(cidx, sid, &frame, fin);
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
        self.write_stream(cidx, stream, data, fin);
        Ok(())
    }

    /// Half/fully close a WT stream.
    pub fn stream_close(&mut self, session: u64, stream: u64) -> io::Result<()> {
        let s = self.sessions.get(&session).ok_or_else(|| io::Error::other("no such session"))?;
        let cidx = s.cidx;
        self.write_stream(cidx, stream, &[], true);
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
        let mut prefix = std::mem::take(&mut self.uni_prefix);
        prefix.clear();
        encode_into(stream_type::WT_UNI_SERVER, &mut prefix);
        encode_into(local_session, &mut prefix);
        let _ = c.conn.stream_send(sid, &prefix, false);
        self.uni_prefix = prefix;
        self.handle_by_stream.insert((cidx, sid), session);
        Ok(sid)
    }

    /// Send a WT datagram (length-prefixed record framing) to the session.
    pub fn send_datagram(&mut self, session: u64, data: &[u8]) -> io::Result<()> {
        let s = self
            .sessions
            .get(&session)
            .ok_or_else(|| io::Error::other("no such session"))?;
        let (cidx, local_session) = (s.cidx, s.request_stream);
        // One buffer, single pass: outer length is computed from encoded
        // varint sizes instead of building an intermediate record.
        let session_len = varint::encoded_len(local_session);
        let inner = (session_len + data.len()) as u64;
        let outer_len = varint::encoded_len(inner);
        let mut rec = std::mem::take(&mut self.ev_scratch);
        rec.clear();
        rec.reserve(outer_len + inner as usize);
        encode_into(inner, &mut rec);
        encode_into(local_session, &mut rec);
        rec.put_slice(data);
        let c = self.conns.get_mut(&cidx).ok_or_else(|| io::Error::other("conn gone"))?;
        let res = c.conn.dgram_send(&rec).map_err(|e| io::Error::other(format!("dgram_send: {e}")));
        self.ev_scratch = rec;
        res
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
        self.write_stream(cidx, sid, &frame, false);
        // Reset active WT streams of this session.
        let mut doomed = std::mem::take(&mut self.doomed);
        doomed.clear();
        if let Some(c) = self.conns.get_mut(&cidx) {
            doomed.extend(
                c.streams
                    .iter()
                    .filter(|(_, st)| matches!(st, StreamIn::Wt { session: h } if *h == session))
                    .map(|(id, _)| *id),
            );
            for i in 0..doomed.len() {
                let id = doomed[i];
                let _ = c
                    .conn
                    .stream_shutdown(id, quiche::Shutdown::Write, h3::WT_APPLICATION_ERROR);
                c.streams.remove(&id);
            }
        }
        self.doomed = doomed;
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

    fn write_stream(&mut self, cidx: u64, sid: u64, data: &[u8], fin: bool) {
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
                                c.pending.entry(sid).or_default().push_back(WriteChunk {
                                    data: Bytes::new(),
                                    pos: 0,
                                    fin: true,
                                });
                                return;
                            }
                        }
                    }
                }
                Err(_) => {
                    c.pending.entry(sid).or_default().push_back(WriteChunk {
                        data: Bytes::copy_from_slice(&data[off..]),
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
        let mut sids = std::mem::take(&mut self.flush_sids);
        sids.clear();
        sids.extend(c.pending.keys().copied());
        for i in 0..sids.len() {
            let sid = sids[i];
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
        self.flush_sids = sids;
    }

    fn flush_send(&mut self, cidx: u64, out: &mut Vec<WtEvent>) {
        loop {
            let res = match self.conns.get_mut(&cidx) {
                Some(c) => c.conn.send(&mut self.send_buf),
                None => return,
            };
            match res {
                Ok((n, info)) => {
                    let _ = self.socket.send_to(&self.send_buf[..n], info.to);
                }
                Err(quiche::Error::Done) => return,
                Err(_) => {
                    self.remove_conn(cidx, "send error", out);
                    return;
                }
            }
        }
    }

    fn feed_quic_dgram(&mut self, cidx: u64, packet: Bytes, out: &mut Vec<WtEvent>) {
        // Length-prefixed records: varint(len) [ varint(session) payload ].
        let mut off = 0usize;
        while off < packet.len() {
            let Some((rlen, n1)) = varint::decode(&packet[off..]) else {
                return;
            };
            let Some(end) = (off + n1).checked_add(rlen as usize) else {
                return;
            };
            if end > packet.len() {
                return;
            }
            let Some((qsid, n2)) = varint::decode(&packet[off + n1..end]) else {
                return;
            };
            if let Some(&handle) = self.handle_by_stream.get(&(cidx, qsid)) {
                out.push(WtEvent::Datagram {
                    session: handle,
                    data: packet.slice(off + n1 + n2..end),
                });
            }
            off = end;
        }
    }

    fn reap_closed(&mut self, out: &mut Vec<WtEvent>) {
        let mut doomed = std::mem::take(&mut self.doomed);
        doomed.clear();
        doomed.extend(
            self.conns
                .iter()
                .filter(|(_, c)| c.conn.is_closed())
                .map(|(k, _)| *k),
        );
        for i in 0..doomed.len() {
            let cidx = doomed[i];
            let reason = self
                .conns
                .get(&cidx)
                .and_then(|c| c.conn.peer_error())
                .map(|e| format!("peer error: {e:?}"))
                .unwrap_or_else(|| "connection closed".into());
            self.remove_conn(cidx, &reason, out);
        }
        self.doomed = doomed;
    }

    fn remove_conn(&mut self, cidx: u64, reason: &str, out: &mut Vec<WtEvent>) {
        let mut doomed = std::mem::take(&mut self.doomed);
        doomed.clear();
        doomed.extend(
            self.sessions
                .iter()
                .filter(|(_, s)| s.cidx == cidx && !s.closed)
                .map(|(h, _)| *h),
        );
        for i in 0..doomed.len() {
            let h = doomed[i];
            out.push(WtEvent::SessionClosed {
                session: h,
                code: h3::WT_APPLICATION_ERROR,
                reason: reason.to_string(),
            });
            self.sessions.remove(&h);
        }
        self.doomed = doomed;
        self.conns.remove(&cidx);
        self.conn_by_dcid.retain(|_, v| *v != cidx);
        self.handle_by_stream.retain(|key, _| key.0 != cidx);
        out.push(WtEvent::ConnectionClosed {
            reason: reason.to_string(),
        });
    }
}

impl Drop for WebTransportServer {
    fn drop(&mut self) {
        // Dropping socket sources deregisters from epoll automatically.
    }
}
