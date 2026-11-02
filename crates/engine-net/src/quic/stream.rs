use std::net::SocketAddr;
use std::time::Instant;

use quinn_proto::{ConnectionHandle, Dir, Event, StreamEvent};

use super::{read_stream, QuicEngine, QuicEvent};

impl QuicEngine {
    pub fn poll_events(&mut self) -> Vec<QuicEvent> {
        let mut events = Vec::new();
        let handles: Vec<ConnectionHandle> = self.connections.keys().copied().collect();
        for handle in handles {
            let Some(conn) = self.connections.get_mut(&handle) else {
                continue;
            };
            loop {
                let Some(endpoint_event) = conn.poll_endpoint_events() else {
                    break;
                };
                if let Some(event) = self.endpoint.handle_event(handle, endpoint_event) {
                    conn.handle_event(event);
                }
            }
            loop {
                let Some(event) = conn.poll() else {
                    break;
                };
                match event {
                    Event::Connected => events.push(QuicEvent::Connected(handle)),
                    Event::ConnectionLost { reason } => {
                        events.push(QuicEvent::Lost(handle, reason));
                    }
                    Event::Stream(StreamEvent::Opened { dir }) => {
                        if dir == Dir::Bi {
                            let mut opened: Vec<quinn_proto::StreamId> = Vec::new();
                            while let Some(id) = conn.streams().accept(Dir::Bi) {
                                opened.push(id);
                            }
                            for id in opened {
                                events.push(QuicEvent::StreamOpened { handle, id });
                                read_stream(conn, handle, id, &mut events);
                            }
                        }
                    }
                    Event::Stream(StreamEvent::Readable { id }) => {
                        read_stream(conn, handle, id, &mut events);
                    }
                    Event::Stream(StreamEvent::Finished { id }) => {
                        events.push(QuicEvent::StreamFinished { handle, id });
                    }
                    Event::DatagramReceived => {
                        while let Some(bytes) = conn.datagrams().recv() {
                            events.push(QuicEvent::Datagram {
                                handle,
                                bytes: bytes.to_vec(),
                            });
                        }
                    }
                    _ => {}
                }
            }
        }
        events
    }

    pub fn first_connection(&self) -> Option<ConnectionHandle> {
        self.connections.keys().next().copied()
    }

    pub fn open_bi(&mut self, handle: ConnectionHandle) -> Option<quinn_proto::StreamId> {
        self.connections
            .get_mut(&handle)
            .and_then(|conn| conn.streams().open(Dir::Bi))
    }

    pub fn stream_write(
        &mut self,
        handle: ConnectionHandle,
        id: quinn_proto::StreamId,
        bytes: &[u8],
    ) -> bool {
        self.connections
            .get_mut(&handle)
            .map(|conn| conn.send_stream(id).write(bytes).is_ok())
            .unwrap_or(false)
    }

    pub fn stream_finish(&mut self, handle: ConnectionHandle, id: quinn_proto::StreamId) -> bool {
        self.connections
            .get_mut(&handle)
            .map(|conn| conn.send_stream(id).finish().is_ok())
            .unwrap_or(false)
    }

    pub fn connection_close(&mut self, now: Instant, handle: ConnectionHandle, reason: &str) -> bool {
        self.connections
            .get_mut(&handle)
            .map(|conn| {
                conn.close(
                    now,
                    0u32.into(),
                    bytes::Bytes::copy_from_slice(reason.as_bytes()),
                );
                true
            })
            .unwrap_or(false)
    }

    pub fn remote_addr(&self, handle: ConnectionHandle) -> Option<SocketAddr> {
        self.connections
            .get(&handle)
            .map(|conn| conn.remote_address())
    }

    pub fn datagram_send(&mut self, handle: ConnectionHandle, bytes: Vec<u8>) -> bool {
        self.connections
            .get_mut(&handle)
            .map(|conn| conn.datagrams().send(bytes.into(), true).is_ok())
            .unwrap_or(false)
    }
}
