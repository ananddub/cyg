//! # Raw TCP Transport
//!
//! Non-blocking raw TCP with length-prefixed binary framing:
//! [2 bytes: len][1 byte: opcode][payload bytes]

use std::collections::HashMap;
use std::io;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use mio::net::TcpListener;
use mio::{Events, Interest, Poll, Token};
use socket2::{Domain, Protocol, Socket, Type};

use super::{GameTransport, TransportEvent};
use crate::connection::ClientConn;
use crate::framing::OpCode;

const LISTENER_TOKEN: Token = Token(0);
const CLIENT_TOKEN_START: usize = 1;

pub struct RawTcpTransport {
    poll: Poll,
    events: Events,
    listener: TcpListener,
    clients: HashMap<Token, ClientConn>,
    client_tokens: HashMap<String, Token>,
    /// Tokens queued for removal during a poll; persists across polls so the
    /// hot loop never allocates. The id is moved out of the connection at
    /// removal, so no id clones happen here.
    disconnected: Vec<Token>,
    next_token: usize,
    bind_addr: SocketAddr,
}

impl RawTcpTransport {
    pub fn new(bind_addr: SocketAddr) -> io::Result<Self> {
        let poll = Poll::new()?;
        let events = Events::with_capacity(1024);

        let socket = Socket::new(Domain::IPV4, Type::STREAM, Some(Protocol::TCP))?;
        socket.set_reuse_address(true)?;
        #[cfg(all(unix, not(target_os = "solaris"), not(target_os = "illumos")))]
        socket.set_reuse_port(true)?;
        socket.set_nonblocking(true)?;
        socket.set_tcp_nodelay(true)?;

        let sock_addr = bind_addr.into();
        socket.bind(&sock_addr)?;
        socket.listen(1024)?;

        let std_listener: std::net::TcpListener = socket.into();
        let mut listener = TcpListener::from_std(std_listener);

        poll.registry()
            .register(&mut listener, LISTENER_TOKEN, Interest::READABLE)?;

        Ok(Self {
            poll,
            events,
            listener,
            clients: HashMap::new(),
            client_tokens: HashMap::new(),
            disconnected: Vec::new(),
            next_token: CLIENT_TOKEN_START,
            bind_addr,
        })
    }
}

impl GameTransport for RawTcpTransport {
    fn poll_events(
        &mut self,
        timeout: Duration,
        events_out: &mut Vec<TransportEvent>,
    ) -> io::Result<()> {
        self.poll.poll(&mut self.events, Some(timeout))?;

        let (disconnected, clients) = (&mut self.disconnected, &mut self.clients);
        for event in self.events.iter() {
            match event.token() {
                LISTENER_TOKEN => {
                    loop {
                        match self.listener.accept() {
                            Ok((mut stream, peer)) => {
                                let token = Token(self.next_token);
                                self.next_token += 1;
                                stream.set_nodelay(true)?;

                                self.poll.registry().register(
                                    &mut stream,
                                    token,
                                    Interest::READABLE | Interest::WRITABLE,
                                )?;

                                let client_id: Arc<str> =
                                    format!("tcp_{}_{}", token.0, peer.port()).into();
                                self.client_tokens.insert(client_id.to_string(), token);
                                clients.insert(token, ClientConn::new(stream, client_id.clone()));

                                events_out.push(TransportEvent::Connected {
                                    client_id,
                                    peer_addr: peer,
                                });
                            }
                            Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => break,
                            Err(e) => return Err(e),
                        }
                    }
                }
                token => {
                    let mut conn_dead = false;

                    if let Some(conn) = clients.get_mut(&token) {
                        if event.is_readable() {
                            match conn.do_read() {
                                Ok(0) => conn_dead = true,
                                Ok(_) => {
                                    while let Some((opcode, payload)) = conn.next_frame() {
                                        events_out.push(TransportEvent::Packet {
                                            client_id: conn.session_id.clone(),
                                            opcode,
                                            payload,
                                        });
                                    }
                                }
                                Err(_) => conn_dead = true,
                            }
                        }

                        if event.is_writable() && conn.has_pending_writes() {
                            let _ = conn.do_write();
                        }
                    }

                    if conn_dead {
                        disconnected.push(token);
                    }
                }
            }
        }

        for token in disconnected.drain(..) {
            if let Some(mut conn) = clients.remove(&token) {
                let _ = self.poll.registry().deregister(&mut conn.stream);
                self.client_tokens.remove(&*conn.session_id);
                events_out.push(TransportEvent::Disconnected {
                    client_id: conn.session_id,
                });
            }
        }

        Ok(())
    }

    fn send_to(&mut self, client_id: &str, opcode: OpCode, payload: &[u8]) -> io::Result<()> {
        if let Some(&token) = self.client_tokens.get(client_id) {
            if let Some(conn) = self.clients.get_mut(&token) {
                conn.queue_message(opcode, payload);
            }
        }
        Ok(())
    }

    fn broadcast(&mut self, opcode: OpCode, payload: &[u8]) -> io::Result<()> {
        for conn in self.clients.values_mut() {
            conn.queue_message(opcode, payload);
        }
        Ok(())
    }

    fn flush(&mut self) -> io::Result<()> {
        for conn in self.clients.values_mut() {
            if conn.has_pending_writes() {
                let _ = conn.do_write();
            }
        }
        Ok(())
    }

    fn ccu(&self) -> usize {
        self.clients.len()
    }

    fn local_addr(&self) -> io::Result<SocketAddr> {
        Ok(self.bind_addr)
    }

    fn transport_name(&self) -> &'static str {
        "Raw TCP"
    }
}
