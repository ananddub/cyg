use std::collections::{HashMap, VecDeque};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Instant;

use bytes::BytesMut;
use quinn_proto::{
    ClientConfig, Connection, ConnectionHandle, DatagramEvent, Endpoint, EndpointConfig,
    ServerConfig, Transmit,
};

use super::QuicEngine;

impl QuicEngine {
    pub fn server(endpoint_config: EndpointConfig, server_config: ServerConfig) -> Self {
        Self::new(
            endpoint_config,
            Some(server_config),
            HashMap::new(),
            VecDeque::new(),
        )
    }

    pub fn client(endpoint_config: EndpointConfig) -> Self {
        Self::new(endpoint_config, None, HashMap::new(), VecDeque::new())
    }

    fn new(
        endpoint_config: EndpointConfig,
        server_config: Option<ServerConfig>,
        connections: HashMap<ConnectionHandle, Connection>,
        pending: VecDeque<quinn_proto::Incoming>,
    ) -> Self {
        let server_config = server_config.map(Arc::new);
        Self {
            endpoint: Endpoint::new(
                Arc::new(endpoint_config),
                server_config.clone(),
                false,
                None,
            ),
            server_config,
            connections,
            pending,
            outbox: VecDeque::new(),
            buf: Vec::with_capacity(1500),
        }
    }

    pub fn connect(
        &mut self,
        now: Instant,
        config: ClientConfig,
        remote: SocketAddr,
        server_name: &str,
    ) -> Result<ConnectionHandle, quinn_proto::ConnectError> {
        let (handle, conn) = self.endpoint.connect(now, config, remote, server_name)?;
        self.connections.insert(handle, conn);
        Ok(handle)
    }

    pub fn handle_datagram(&mut self, now: Instant, from: SocketAddr, data: &[u8]) {
        self.buf.clear();
        match self
            .endpoint
            .handle(now, from, None, None, BytesMut::from(data), &mut self.buf)
        {
            Some(DatagramEvent::Response(transmit)) => self.push_transmit(transmit),
            Some(DatagramEvent::ConnectionEvent(handle, event)) => {
                if let Some(conn) = self.connections.get_mut(&handle) {
                    conn.handle_event(event);
                }
            }
            Some(DatagramEvent::NewConnection(incoming)) => {
                self.pending.push_back(incoming);
            }
            None => {}
        }
    }

    pub fn accept_pending(&mut self, now: Instant) -> Vec<ConnectionHandle> {
        let mut accepted = Vec::new();
        while let Some(incoming) = self.pending.pop_front() {
            match self
                .endpoint
                .accept(incoming, now, &mut self.buf, self.server_config.clone())
            {
                Ok((handle, conn)) => {
                    self.connections.insert(handle, conn);
                    accepted.push(handle);
                }

                Err(_) => continue,
            }
        }
        accepted
    }

    pub fn next_timeout(&mut self) -> Option<Instant> {
        self.connections
            .values_mut()
            .filter_map(|conn| conn.poll_timeout())
            .min()
    }

    pub fn handle_timeout(&mut self, now: Instant) {
        for conn in self.connections.values_mut() {
            if conn.poll_timeout().is_some_and(|at| at <= now) {
                conn.handle_timeout(now);
            }
        }
    }

    pub fn poll_outbound(&mut self, now: Instant) -> Option<(SocketAddr, Vec<u8>)> {
        if let Some(pair) = self.outbox.pop_front() {
            return Some(pair);
        }
        for conn in self.connections.values_mut() {
            self.buf.clear();
            if let Some(transmit) = conn.poll_transmit(now, 1, &mut self.buf) {
                let pair = (transmit.destination, self.buf[..transmit.size].to_vec());
                return Some(pair);
            }
        }
        None
    }

    fn push_transmit(&mut self, transmit: Transmit) {
        self.outbox
            .push_back((transmit.destination, self.buf[..transmit.size].to_vec()));
    }
}
