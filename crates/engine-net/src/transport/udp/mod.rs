use std::collections::HashMap;
use std::io;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Instant;

use bytes::{Bytes, BytesMut};

use mio::net::UdpSocket;
use mio::{Events, Interest, Poll, Token};
use socket2::{Domain, Protocol, Socket, Type};

mod transport;

const UDP_SOCKET_TOKEN: Token = Token(0);

struct UdpPeer {
    client_id: Arc<str>,
    last_seen: Instant,
}

pub struct UdpTransport {
    poll: Poll,
    events: Events,
    socket: UdpSocket,
    peers: HashMap<SocketAddr, UdpPeer>,
    peer_addrs: HashMap<String, SocketAddr>,
    bind_addr: SocketAddr,
    recv_buf: BytesMut,
    send_queue: Vec<(SocketAddr, Bytes)>,

    sb: BytesMut,

    timed_out: Vec<(SocketAddr, Arc<str>)>,
}

impl UdpTransport {
    pub fn new(bind_addr: SocketAddr) -> io::Result<Self> {
        let poll = Poll::new()?;
        let events = Events::with_capacity(1024);

        let socket = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP))?;
        socket.set_reuse_address(true)?;
        #[cfg(all(unix, not(target_os = "solaris"), not(target_os = "illumos")))]
        socket.set_reuse_port(true)?;
        socket.set_nonblocking(true)?;

        let sock_addr = bind_addr.into();
        socket.bind(&sock_addr)?;

        let std_socket: std::net::UdpSocket = socket.into();
        let mut mio_socket = UdpSocket::from_std(std_socket);

        poll.registry().register(
            &mut mio_socket,
            UDP_SOCKET_TOKEN,
            Interest::READABLE | Interest::WRITABLE,
        )?;

        Ok(Self {
            poll,
            events,
            socket: mio_socket,
            peers: HashMap::new(),
            peer_addrs: HashMap::new(),
            bind_addr,
            recv_buf: BytesMut::with_capacity(65536),
            send_queue: Vec::new(),
            sb: BytesMut::with_capacity(1024),
            timed_out: Vec::new(),
        })
    }
}
