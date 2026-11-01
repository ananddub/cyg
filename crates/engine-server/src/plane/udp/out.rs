use std::io;
use std::net::SocketAddr;

use engine_bus::{FrameKind, OutFrame};
use engine_webrtc_protocol::EventFrame;
use mio::net::UdpSocket;

use crate::plane::mono_ns;
use crate::plane::udp::plane::{OP_DENY, SEND_RETRIES, UdpPlane};

impl UdpPlane {
    pub fn send_frame(&mut self, frame: &OutFrame) -> bool {
        if frame.kind == FrameKind::Reliable {
            return self.queue_reliable(&frame.peer_id, &frame.bytes);
        }
        let Some(&addr) = self.peer_addrs.get(frame.peer_id.as_str()) else {
            return false;
        };
        let mut sock = self.socket.borrow_mut();
        match Self::send_raw(&mut sock, &frame.bytes, addr) {
            Ok(()) => {
                self.sends += 1;
                true
            }
            Err(_) => {
                self.send_drops += 1;
                false
            }
        }
    }

    pub(super) fn queue_reliable(&mut self, peer_id: &str, bytes: &[u8]) -> bool {
        let Ok(event) = EventFrame::decode(bytes) else {
            self.reliable_refused += 1;
            return false;
        };
        let Some(&addr) = self.peer_addrs.get(peer_id) else {
            self.reliable_refused += 1;
            return false;
        };
        let now = mono_ns() / 1_000_000;
        let refused = {
            let Some(peer) = self.peers.get_mut(&addr) else {
                self.reliable_refused += 1;
                return false;
            };
            if !peer.bound {
                self.reliable_refused += 1;
                return false;
            }
            peer.reliable.queue(event.seq, bytes.to_vec(), now).is_err()
        };
        if refused {
            self.reliable_refused += 1;
            return false;
        }
        self.pump_peer(addr, now);
        true
    }

    pub(super) fn send_deny(&mut self, addr: SocketAddr, reason: u8) {
        let mut sock = self.socket.borrow_mut();
        Self::send_deny_raw(&mut sock, &mut self.scratch, &mut self.denied, addr, reason);
    }

    pub(super) fn send_deny_raw(
        sock: &mut UdpSocket,
        scratch: &mut Vec<u8>,
        denied: &mut u64,
        addr: SocketAddr,
        reason: u8,
    ) {
        scratch.clear();
        scratch.push(OP_DENY);
        scratch.push(reason);
        if sock.send_to(scratch, addr).is_ok() {
            *denied += 1;
        }
    }

    pub(super) fn send_raw(
        sock: &mut UdpSocket,
        bytes: &[u8],
        addr: SocketAddr,
    ) -> io::Result<()> {
        let mut tries: u8 = 0;
        loop {
            match sock.send_to(bytes, addr) {
                Ok(_) => return Ok(()),
                Err(ref e) if e.kind() == io::ErrorKind::WouldBlock && tries < SEND_RETRIES => {
                    tries += 1;
                }
                Err(e) => return Err(e),
            }
        }
    }
}
