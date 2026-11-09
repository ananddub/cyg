use std::cell::RefCell;
use std::collections::HashMap;
use std::io;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use mio::net::UdpSocket;
use mio::{Interest, Token};

use crate::plane::seats::SeatRegistry;
use crate::plane::udp::bucket::TokenBucket;
use crate::plane::udp::peer::Peer;

pub const UDP_TOKEN: Token = Token(1);
pub const MAX_PACKETS_PER_SEC: u32 = 512;

pub(super) const OP_JOIN: u8 = 0x20;
pub(super) const OP_INP: u8 = 0x21;
pub(super) const OP_PING: u8 = 0x22;
pub(super) const OP_PONG: u8 = 0x23;
pub(super) const OP_BIND: u8 = 0x24;
pub(super) const OP_DENY: u8 = 0x25;

pub(super) const DENY_BAD_TOKEN: u8 = 1;
pub(super) const DENY_FULL: u8 = 4;

pub(super) const INP_LEN: usize = 23;
pub(super) const SEND_RETRIES: u8 = 3;

pub struct UdpPlane {
    pub(super) socket: RefCell<UdpSocket>,
    pub(super) bus: Arc<engine_bus::Bus>,
    pub(super) seats: Arc<SeatRegistry>,
    pub(super) peers: HashMap<SocketAddr, Peer>,
    pub(super) peer_addrs: HashMap<engine_bus::PeerId, SocketAddr>,
    pub(super) strangers: HashMap<SocketAddr, TokenBucket>,
    pub(super) rx_buf: Vec<u8>,
    pub(super) scratch: Vec<u8>,
    pub(super) session_counter: u64,
    pub(super) tick_hz: f64,
    pub(super) peer_idle: Duration,
    pub(super) pkts_in: u64,
    pub(super) denied: u64,
    pub(super) rate_limited: u64,
    pub(super) sends: u64,
    pub(super) send_drops: u64,
    pub(super) recv_errors: u64,
    pub(super) reliable_refused: u64,
    pub(super) acks_ignored: u64,
    pub(super) events_in: u64,
    pub(super) events_acked: u64,
}

impl UdpPlane {
    pub fn new(
        bind_addr: &str,
        bus: Arc<engine_bus::Bus>,
        seats: Arc<SeatRegistry>,
        peer_idle: Duration,
    ) -> io::Result<UdpPlane> {
        let addr: SocketAddr = bind_addr
            .parse()
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?;
        let std_socket = crate::plane::udp::bind::bind_udp(addr)?;
        let socket = RefCell::new(UdpSocket::from_std(std_socket));
        let tick_hz = std::env::var("CYG_TICK_HZ")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(128.0);
        Ok(UdpPlane {
            socket,
            bus,
            seats,
            peers: HashMap::new(),
            peer_addrs: HashMap::new(),
            strangers: HashMap::new(),
            rx_buf: vec![0u8; 65536],
            scratch: Vec::with_capacity(128),
            session_counter: 0,
            tick_hz,
            peer_idle,
            pkts_in: 0,
            denied: 0,
            rate_limited: 0,
            sends: 0,
            send_drops: 0,
            recv_errors: 0,
            reliable_refused: 0,
            acks_ignored: 0,
            events_in: 0,
            events_acked: 0,
        })
    }

    pub fn local_addr(&self) -> io::Result<SocketAddr> {
        self.socket.borrow().local_addr()
    }

    pub fn set_tick_hz(&mut self, hz: f64) {
        self.tick_hz = hz;
    }

    pub fn register(&self, poll: &mio::Poll) -> io::Result<()> {
        let mut sock = self.socket.borrow_mut();
        poll.registry()
            .register(&mut *sock, UDP_TOKEN, Interest::READABLE)
    }
}
