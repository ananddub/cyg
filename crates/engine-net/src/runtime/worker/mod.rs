
mod net;
mod r#loop;

use std::collections::HashMap;
use std::io;
use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use bytes::Bytes;
use mio::net::TcpListener;
use mio::{Events, Interest, Poll, Token};
use socket2::{Domain, Protocol, Socket, Type};

use crate::framing::OpCode;
use crate::runtime::connection::ClientConn;
use engine_core::FixedClock;

pub(super) const LISTENER_TOKEN: Token = Token(0);
pub(super) const WAKER_TOKEN: Token = Token(1);
pub(super) const CLIENT_TOKEN_START: usize = 2;

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

#[derive(Debug, Default)]
pub struct WorkerStats {
    pub ccu: AtomicUsize,
    pub total_packets_in: AtomicU64,
    pub total_packets_out: AtomicU64,
    pub last_tick_time_us: AtomicU64,
    pub total_ticks: AtomicU64,
}

#[derive(Debug)]
pub struct ClientPacket {

    pub session_id: Arc<str>,
    pub token: Token,
    pub opcode: OpCode,

    pub payload: Bytes,
}

pub struct MioWorker {
    pub(super) poll: Poll,
    pub(super) events: Events,
    pub(super) listener: TcpListener,
    pub(super) clock: FixedClock,
    pub(super) clients: HashMap<Token, ClientConn>,
    pub(super) next_token: usize,
    running: Arc<AtomicBool>,
    pub(super) next_tick_deadline: Instant,
    pub(super) stats: Arc<WorkerStats>,

    pub(super) disconnected_tokens: Vec<Token>,
}

impl MioWorker {

    pub fn new(cfg: WorkerConfig) -> io::Result<Self> {
        let poll = Poll::new()?;
        let events = Events::with_capacity(1024);

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

    pub fn stats(&self) -> Arc<WorkerStats> {
        self.stats.clone()
    }

    pub fn running_flag(&self) -> Arc<AtomicBool> {
        self.running.clone()
    }

    pub fn active_connection_count(&self) -> usize {
        self.clients.len()
    }

    pub fn broadcast(&mut self, opcode: OpCode, payload: &[u8]) {
        let count = self.clients.len();
        for conn in self.clients.values_mut() {
            conn.queue_message(opcode, payload);
        }
        self.stats
            .total_packets_out
            .fetch_add(count as u64, Ordering::Relaxed);
    }

    pub(super) fn tick_budget(&self, now: Instant, max_wait: Duration) -> Duration {
        if self.next_tick_deadline > now {
            (self.next_tick_deadline - now).min(max_wait)
        } else {
            Duration::from_millis(0)
        }
    }
}
