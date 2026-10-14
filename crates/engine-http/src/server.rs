//! # HTTP/1.1 Backend (raw mio)
//!
//! A non-blocking `Poll` + `SO_REUSEPORT` listener that parses requests with
//! [`RequestDecoder`](crate::parse::RequestDecoder), dispatches them through a
//! shared [`Router`](crate::router::Router), and writes responses. Runs either
//! inline (driven by `poll`) or on a dedicated thread via [`ServerHandle`].

use std::collections::HashMap;
use std::io::{self, Read, Write};
use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use mio::net::TcpStream;
use mio::{Events, Interest, Poll, Token};
use socket2::{Domain, Protocol, Socket, Type};

use crate::backend::HttpVersion;
use crate::parse::{ParseOutcome, RequestDecoder};
use crate::router::Router;
use crate::types::Response;
use crate::writer::serialize_http1_into;

const LISTENER_TOKEN: Token = Token(0);

struct Conn {
    stream: TcpStream,
    decoder: RequestDecoder,
    peer: SocketAddr,
}

/// The concrete HTTP/1.1 transport.
pub struct Http1Backend {
    poll: Poll,
    events: Events,
    listener: mio::net::TcpListener,
    bind_addr: SocketAddr,
    router: Arc<Router>,
    conns: HashMap<usize, Conn>,
    next_token: usize,
    served: Arc<AtomicU64>,
    /// Reused across polls so the ready-set scan allocates nothing per tick.
    ready: Vec<usize>,
    closing: Vec<usize>,
    resp_buf: Vec<u8>,
}

impl Http1Backend {
    pub fn new(bind_addr: SocketAddr, router: Arc<Router>) -> io::Result<Self> {
        let poll = Poll::new()?;
        let events = Events::with_capacity(512);

        let socket = Socket::new(Domain::IPV4, Type::STREAM, Some(Protocol::TCP))?;
        socket.set_reuse_address(true)?;
        #[cfg(all(unix, not(target_os = "solaris"), not(target_os = "illumos")))]
        socket.set_reuse_port(true)?;
        socket.set_nonblocking(true)?;
        socket.bind(&bind_addr.into())?;
        socket.listen(1024)?;

        let std_listener: std::net::TcpListener = socket.into();
        let mut listener = mio::net::TcpListener::from_std(std_listener);
        poll.registry()
            .register(&mut listener, LISTENER_TOKEN, Interest::READABLE)?;

        let actual_addr = listener.local_addr().unwrap_or(bind_addr);

        Ok(Self {
            poll,
            events,
            listener,
            bind_addr: actual_addr,
            router,
            conns: HashMap::new(),
            next_token: 1,
            served: Arc::new(AtomicU64::new(0)),
            ready: Vec::with_capacity(64),
            closing: Vec::with_capacity(8),
            resp_buf: Vec::with_capacity(4096),
        })
    }

    fn accept_pending(&mut self) -> io::Result<()> {
        loop {
            match self.listener.accept() {
                Ok((mut stream, peer)) => {
                    let _ = stream.set_nodelay(true);
                    let token = Token(self.next_token);
                    self.next_token += 1;
                    self.poll.registry().register(
                        &mut stream,
                        token,
                        Interest::READABLE | Interest::WRITABLE,
                    )?;
                    self.conns.insert(
                        token.0,
                        Conn {
                            stream,
                            decoder: RequestDecoder::new(),
                            peer,
                        },
                    );
                }
                Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => break,
                Err(e) => return Err(e),
            }
        }
        Ok(())
    }

    /// Read whatever is available from a connection, serving each complete
    /// request. Returns Ok(false) when the connection should be dropped.
    fn drain_connection(&mut self, id: usize) -> io::Result<bool> {
        let conn = match self.conns.get_mut(&id) {
            Some(c) => c,
            None => return Ok(true),
        };

        let mut chunk = [0u8; 8192];
        let peer = conn.peer;
        loop {
            match conn.stream.read(&mut chunk) {
                Ok(0) => return Ok(false),
                Ok(n) => {
                    conn.decoder.feed(&chunk[..n]);
                    match conn.decoder.next_request() {
                        ParseOutcome::Complete {
                            mut request,
                            keep_alive,
                        } => {
                            request
                                .headers
                                .insert("x-peer-addr", peer.to_string());
                            let head_only = request.method == crate::types::Method::Head;
                            let response = self.router.dispatch(request);
                            let body_len = response.body.len();
                            let buf = &mut self.resp_buf;
                            serialize_http1_into(buf, &response, keep_alive, body_len, head_only);
                            write_all_best_effort(&mut conn.stream, buf);
                            self.served.fetch_add(1, Ordering::Relaxed);
                            if !keep_alive {
                                return Ok(false);
                            }
                            // Keep parsing for pipelined/sequential requests.
                        }
                        ParseOutcome::NeedMore => break,
                        ParseOutcome::Failed(status, reason) => {
                            let resp = Response::error(status, reason);
                            let buf = &mut self.resp_buf;
                            serialize_http1_into(buf, &resp, false, resp.body.len(), false);
                            write_all_best_effort(&mut conn.stream, buf);
                            return Ok(false);
                        }
                    }
                }
                Err(ref e)
                    if e.kind() == io::ErrorKind::WouldBlock
                        || e.kind() == io::ErrorKind::Interrupted =>
                {
                    break;
                }
                Err(_) => return Ok(false),
            }
        }
        Ok(true)
    }
}

impl crate::backend::HttpBackend for Http1Backend {
    fn poll(&mut self, timeout: Duration) -> io::Result<usize> {
        let before = self.served.load(Ordering::Relaxed);
        self.poll.poll(&mut self.events, Some(timeout))?;
        self.accept_pending()?;

        // Adopted scratch buffers: capacity survives between polls.
        let mut ready = std::mem::take(&mut self.ready);
        let mut closing = std::mem::take(&mut self.closing);
        ready.clear();
        closing.clear();

        for e in self.events.iter() {
            if e.token() != LISTENER_TOKEN && e.is_readable() {
                ready.push(e.token().0);
            }
        }

        for k in 0..ready.len() {
            let id = ready[k];
            if !self.drain_connection(id)? {
                close_conn(&mut self.poll, &mut self.conns, id);
            }
        }

        for e in self.events.iter() {
            if e.token() != LISTENER_TOKEN && e.is_writable() {
                let id = e.token().0;
                if let Some(conn) = self.conns.get_mut(&id) {
                    let mut sink = [0u8; 1];
                    // Reads on writable signal EOF too.
                    if let Ok(0) = conn.stream.read(&mut sink) {
                        closing.push(id);
                    }
                }
            }
        }

        for k in 0..closing.len() {
            close_conn(&mut self.poll, &mut self.conns, closing[k]);
        }

        self.ready = ready;
        self.closing = closing;
        Ok((self.served.load(Ordering::Relaxed) - before) as usize)
    }

    fn local_addr(&self) -> io::Result<SocketAddr> {
        Ok(self.bind_addr)
    }

    fn version(&self) -> HttpVersion {
        HttpVersion::Http1
    }

    fn requests_served(&self) -> u64 {
        self.served.load(Ordering::Relaxed)
    }
}

fn close_conn(poll: &mut Poll, conns: &mut HashMap<usize, Conn>, id: usize) {
    if let Some(mut conn) = conns.remove(&id) {
        let _ = poll.registry().deregister(&mut conn.stream);
        let _ = conn.stream.shutdown(std::net::Shutdown::Both);
    }
}

/// Best-effort write of the full buffer, tolerating WouldBlock (rare for
/// small responses given we register writable).
fn write_all_best_effort(stream: &mut TcpStream, buf: &[u8]) {
    let mut off = 0;
    while off < buf.len() {
        match stream.write(&buf[off..]) {
            Ok(0) => break,
            Ok(n) => off += n,
            Err(ref e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                // Small API responses almost never fill the kernel buffer;
                // any remainder is dropped to avoid blocking the tick loop.
                break;
            }
            Err(_) => break,
        }
    }
}

/// Handle to a background HTTP server thread.
pub struct ServerHandle {
    running: Arc<AtomicBool>,
    thread: Mutex<Option<JoinHandle<()>>>,
    addr: SocketAddr,
    served: Arc<AtomicU64>,
}

impl ServerHandle {
    /// Spawn the backend on its own thread. Binds synchronously first so the
    /// returned handle reports the real listening address.
    pub fn spawn(mut backend: Box<dyn crate::backend::HttpBackend>) -> io::Result<Self> {
        let addr = backend.local_addr()?;
        let served_counter = Arc::new(AtomicU64::new(0));
        let served_clone = served_counter.clone();
        let running = Arc::new(AtomicBool::new(true));
        let run_clone = running.clone();

        let thread = thread::Builder::new()
            .name("http-server".to_string())
            .spawn(move || {
                let mut poll_tick = Duration::from_millis(2);
                while run_clone.load(Ordering::Relaxed) {
                    let n = backend.poll(poll_tick).unwrap_or_default();
                    if n > 0 {
                        served_clone.fetch_add(n as u64, Ordering::Relaxed);
                    }
                    // Adaptive idle backoff keeps the thread cheap when quiet.
                    poll_tick = if n == 0 {
                        (poll_tick * 2).min(Duration::from_millis(50))
                    } else {
                        Duration::from_millis(2)
                    };
                }
            })?;

        Ok(Self {
            running,
            thread: Mutex::new(Some(thread)),
            addr,
            served: served_counter,
        })
    }

    pub fn local_addr(&self) -> SocketAddr {
        self.addr
    }

    pub fn requests_served(&self) -> u64 {
        self.served.load(Ordering::Relaxed)
    }

    pub fn stop(&self) {
        self.running.store(false, Ordering::Relaxed);
    }

    /// Signal stop and join the server thread.
    pub fn shutdown(self) {
        self.running.store(false, Ordering::Relaxed);
        if let Ok(mut guard) = self.thread.lock() {
            if let Some(handle) = guard.take() {
                let _ = handle.join();
            }
        }
    }
}

impl Drop for ServerHandle {
    fn drop(&mut self) {
        self.running.store(false, Ordering::Relaxed);
    }
}
