//! # Raw Mio Thread-Per-Core Worker
//!
//! Event loop running on a single pinned CPU core:
//! - Bound with SO_REUSEPORT for kernel-level accept distribution
//! - Microsecond epoll wait bounded by 60Hz tick deadline
//! - Batched reads, room updates, and batched write flushes

use bytes::Bytes;
use std::collections::HashMap;
use std::io;
use std::net::SocketAddr;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::time::{Duration, Instant};

use mio::net::TcpListener;
use mio::{Events, Interest, Poll, Token};
use socket2::{Domain, Protocol, Socket, Type};

use crate::connection::ClientConn;
use crate::framing::OpCode;
use engine_core::{FixedClock, StepContext};

const LISTENER_TOKEN: Token = Token(0);
const WAKER_TOKEN: Token = Token(1);
const CLIENT_TOKEN_START: usize = 2;

/// Configuration for a Mio worker thread.
#[derive(Debug, Clone)]
pub struct WorkerConfig {
    pub bind_addr: SocketAddr,
    pub tick_rate: u32,
    pub max_clients: usize,
}

impl Default for WorkerConfig {
    fn default() -> Self {
        Self {
            bind_addr: "127.0.0.1:8080".parse().unwrap(),
            tick_rate: 60,
            max_clients: 4096,
        }
    }
}

/// Real-time metrics tracked by an individual worker.
#[derive(Debug, Default)]
pub struct WorkerStats {
    pub ccu: std::sync::atomic::AtomicUsize,
    pub total_packets_in: std::sync::atomic::AtomicU64,
    pub total_packets_out: std::sync::atomic::AtomicU64,
    pub last_tick_time_us: std::sync::atomic::AtomicU64,
    pub total_ticks: std::sync::atomic::AtomicU64,
}

/// Incoming client packet event dispatched to game rooms.
#[derive(Debug)]
pub struct ClientPacket {
    /// Refcounted id — cloning this per packet is a refcount bump, not a copy.
    pub session_id: Arc<str>,
    pub token: Token,
    pub opcode: OpCode,
    /// Zero-copy view into the connection's read buffer — cloning this
    /// is a refcount bump, not a byte copy.
    pub payload: Bytes,
}

/// High-performance Mio Worker event loop.
pub struct MioWorker {
    poll: Poll,
    events: Events,
    listener: TcpListener,
    clock: FixedClock,
    clients: HashMap<Token, ClientConn>,
    next_token: usize,
    running: Arc<AtomicBool>,
    next_tick_deadline: Instant,
    stats: Arc<WorkerStats>,
    /// Persistent scratch: tokens whose connections dropped this step.
    disconnected_tokens: Vec<Token>,
}

impl MioWorker {
    /// Bind a new worker listener using SO_REUSEPORT.
    pub fn new(cfg: WorkerConfig) -> io::Result<Self> {
        let poll = Poll::new()?;
        let events = Events::with_capacity(1024);

        // Configure TCP socket with SO_REUSEPORT and SO_REUSEADDR
        let socket = Socket::new(Domain::IPV4, Type::STREAM, Some(Protocol::TCP))?;
        socket.set_reuse_address(true)?;
        #[cfg(all(unix, not(target_os = "solaris"), not(target_os = "illumos")))]
        socket.set_reuse_port(true)?;
        socket.set_nonblocking(true)?;
        socket.set_tcp_nodelay(true)?;

        let socket_addr = cfg.bind_addr.into();
        socket.bind(&socket_addr)?;
        socket.listen(1024)?;

        let std_listener: std::net::TcpListener = socket.into();
        let mut listener = TcpListener::from_std(std_listener);

        poll.registry()
            .register(&mut listener, LISTENER_TOKEN, Interest::READABLE)?;

        let clock = FixedClock::new(cfg.tick_rate);
        let next_tick_deadline = Instant::now() + clock.step_duration;

        Ok(Self {
            poll,
            events,
            listener,
            clock,
            clients: HashMap::new(),
            next_token: CLIENT_TOKEN_START,
            running: Arc::new(AtomicBool::new(true)),
            next_tick_deadline,
            stats: Arc::new(WorkerStats::default()),
            disconnected_tokens: Vec::new(),
        })
    }

    /// Access the worker metrics handle.
    pub fn stats(&self) -> Arc<WorkerStats> {
        self.stats.clone()
    }

    /// Access the running flag (can be set to false from another thread to gracefully stop).
    pub fn running_flag(&self) -> Arc<AtomicBool> {
        self.running.clone()
    }

    /// Number of active connections currently connected to this worker.
    pub fn active_connection_count(&self) -> usize {
        self.clients.len()
    }

    /// Run a single step/iteration of the worker event loop.
    /// Received packets are appended into the caller-owned `packets` vec,
    /// so the hot loop performs zero per-step Vec allocations.
    pub fn step<F>(
        &mut self,
        max_wait: Duration,
        packets: &mut Vec<ClientPacket>,
        mut on_tick: F,
    ) -> io::Result<()>
    where
        F: FnMut(StepContext, &mut HashMap<Token, ClientConn>),
    {
        packets.clear();
        let now = Instant::now();
        let timeout = if self.next_tick_deadline > now {
            (self.next_tick_deadline - now).min(max_wait)
        } else {
            Duration::from_millis(0)
        };

        // Poll for I/O events with timeout bounded by tick deadline
        self.poll.poll(&mut self.events, Some(timeout))?;

        let mut disconnected = std::mem::take(&mut self.disconnected_tokens);
        disconnected.clear();

        // 1. Process all I/O events
        for event in self.events.iter() {
            match event.token() {
                LISTENER_TOKEN => {
                    // Accept all pending connections
                    loop {
                        match self.listener.accept() {
                            Ok((mut stream, peer_addr)) => {
                                let token = Token(self.next_token);
                                self.next_token += 1;

                                if let Err(e) = stream.set_nodelay(true) {
                                    eprintln!("Failed to set nodelay: {}", e);
                                }

                                if let Err(e) = self.poll.registry().register(
                                    &mut stream,
                                    token,
                                    Interest::READABLE | Interest::WRITABLE,
                                ) {
                                    eprintln!("Failed to register client: {}", e);
                                    continue;
                                }

                                // Allocated exactly once per connection; every
                                // packet referencing it is a refcount bump.
                                let session_id: Arc<str> =
                                    format!("sess_{}_{}", token.0, peer_addr.port()).into();
                                self.clients.insert(token, ClientConn::new(stream, session_id));
                            }
                            Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                                break;
                            }
                            Err(e) => {
                                eprintln!("Accept error: {}", e);
                                break;
                            }
                        }
                    }
                }
                WAKER_TOKEN => {
                    // External waker triggered
                }
                client_token => {
                    let mut conn_disconnected = false;

                    if let Some(conn) = self.clients.get_mut(&client_token) {
                        // Handle reading
                        if event.is_readable() {
                            match conn.do_read() {
                                Ok(0) => {
                                    // Client disconnected
                                    conn_disconnected = true;
                                }
                                Ok(_) => {
                                    // Pull all available frames
                                    while let Some((opcode, payload)) = conn.next_frame() {
                                        packets.push(ClientPacket {
                                            session_id: conn.session_id.clone(),
                                            token: client_token,
                                            opcode,
                                            payload,
                                        });
                                    }
                                }
                                Err(e) => {
                                    eprintln!("Socket read error: {}", e);
                                    conn_disconnected = true;
                                }
                            }
                        }

                        // Handle writing
                        if event.is_writable() && conn.has_pending_writes() {
                            if let Err(e) = conn.do_write() {
                                eprintln!("Socket write error: {}", e);
                                conn_disconnected = true;
                            }
                        }
                    }

                    if conn_disconnected {
                        disconnected.push(client_token);
                    }
                }
            }
        }

        // Clean up disconnected clients
        for token in disconnected.drain(..) {
            if let Some(mut conn) = self.clients.remove(&token) {
                let _ = self.poll.registry().deregister(&mut conn.stream);
            }
        }
        self.disconnected_tokens = disconnected;

        // 2. Fire Game Tick if deadline reached
        let now = Instant::now();
        if now >= self.next_tick_deadline {
            let tick_start = Instant::now();
            let ctx = self.clock.advance();
            self.next_tick_deadline += self.clock.step_duration;
            if self.next_tick_deadline < now {
                self.next_tick_deadline = now + self.clock.step_duration;
            }
            on_tick(ctx, &mut self.clients);

            // 3. Batched Flush: Flush all outgoing bytes across all clients
            for (_, conn) in self.clients.iter_mut() {
                if conn.has_pending_writes() {
                    let _ = conn.do_write();
                }
            }
            let elapsed_us = tick_start.elapsed().as_micros() as u64;
            self.stats.last_tick_time_us.store(elapsed_us, std::sync::atomic::Ordering::Relaxed);
            self.stats.total_ticks.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }

        self.stats.ccu.store(self.clients.len(), std::sync::atomic::Ordering::Relaxed);
        if !packets.is_empty() {
            self.stats.total_packets_in.fetch_add(packets.len() as u64, std::sync::atomic::Ordering::Relaxed);
        }

        Ok(())
    }

    /// Broadcast a message to all connected clients on this worker.
    pub fn broadcast(&mut self, opcode: OpCode, payload: &[u8]) {
        let count = self.clients.len();
        for conn in self.clients.values_mut() {
            conn.queue_message(opcode, payload);
        }
        self.stats.total_packets_out.fetch_add(count as u64, std::sync::atomic::Ordering::Relaxed);
    }
}
