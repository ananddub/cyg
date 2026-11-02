use super::frame_bytes;
use super::QuicTransport;
use crate::quic::QuicEvent;
use crate::transport::webrtc::contract::{classify_inbound, Lane};

impl QuicTransport {
    pub(super) fn handle_engine_event(&mut self, event: QuicEvent) {
        match event {
            QuicEvent::Connected(handle) => {
                let peer = self.engine.remote_addr(handle).unwrap_or_else(|| {
                    std::net::SocketAddr::new(
                        std::net::IpAddr::V4(std::net::Ipv4Addr::UNSPECIFIED),
                        0,
                    )
                });
                self.mint(handle, peer);
            }
            QuicEvent::StreamOpened { handle, id } => {
                let Some(cid) = self.by_handle.get(&handle).cloned() else {
                    return;
                };
                self.reply_stream.insert(cid.clone(), id);

                if let Some(queued) = self.pending_reliable.remove(&cid) {
                    for frame in queued {
                        let framed = frame_bytes(&frame);
                        self.engine.stream_write(handle, id, &framed);
                    }
                }
            }
            QuicEvent::StreamData {
                handle,
                id: _,
                bytes,
            } => {
                let Some(cid) = self.by_handle.get(&handle).cloned() else {
                    return;
                };
                let buffer = self.reassembly.entry(cid.clone()).or_default();
                buffer.extend_from_slice(&bytes);
                loop {
                    if buffer.len() < super::FRAME_HEADER {
                        break;
                    }
                    let len = u32::from_be_bytes([buffer[0], buffer[1], buffer[2], buffer[3]])
                        as usize;
                    if buffer.len() < super::FRAME_HEADER + len {
                        break;
                    }
                    let frame: Vec<u8> =
                        buffer[super::FRAME_HEADER..super::FRAME_HEADER + len].to_vec();
                    buffer.drain(..super::FRAME_HEADER + len);
                    let inbound = classify_inbound(Some(Lane::Reliable), &frame);
                    self.inbound.push((cid.clone(), inbound));
                }
                if buffer.is_empty() {
                    self.reassembly.remove(&cid);
                }
            }
            QuicEvent::StreamFinished { handle, id: _ } => {
                if let Some(cid) = self.by_handle.get(&handle).cloned() {
                    self.reply_stream.remove(&cid);
                }
            }
            QuicEvent::Datagram { handle, bytes } => {
                let Some(cid) = self.by_handle.get(&handle).cloned() else {
                    return;
                };
                let inbound = classify_inbound(Some(Lane::Unreliable), &bytes);
                self.inbound.push((cid, inbound));
            }
            QuicEvent::Lost(handle, _reason) => {
                if let Some(cid) = self.by_handle.remove(&handle) {
                    self.by_client.remove(&cid);
                    self.remote.remove(&cid);
                    self.reply_stream.remove(&cid);
                    self.pending_reliable.remove(&cid);
                    self.reassembly.remove(&cid);
                    self.pending
                        .push(crate::transport::TransportEvent::Disconnected { client_id: cid });
                }
            }
        }
    }
}
