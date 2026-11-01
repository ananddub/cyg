use std::net::SocketAddr;

use engine_bus::{InputMsg, Via};
use engine_webrtc_protocol::parse_ack;

use crate::plane::mono_ns;
use crate::plane::udp::plane::{INP_LEN, OP_PONG, UdpPlane};

impl UdpPlane {
    pub(super) fn handle_inp(&mut self, len: usize, addr: SocketAddr, arrival_ns: u64) {
        if len != INP_LEN {
            return;
        }
        let pid = match self.peers.get(&addr) {
            Some(p) if p.bound => p.player_id,
            _ => return,
        };
        let mut payload = [0u8; 20];
        payload.copy_from_slice(&self.rx_buf[3..23]);
        let seq = u16::from_le_bytes([self.rx_buf[1], self.rx_buf[2]]);
        self.bus.push_input(InputMsg {
            player_id: pid,
            seq,
            payload,
            arrival_ns,
            via: Via::NativeUdp,
        });
    }

    pub(super) fn handle_ping(&mut self, len: usize, addr: SocketAddr) {
        if len < 9 {
            return;
        }
        self.scratch.clear();
        self.scratch.push(OP_PONG);
        self.scratch.extend_from_slice(&self.rx_buf[1..9]);
        let sock = self.socket.borrow_mut();
        if sock.send_to(&self.scratch, addr).is_ok() {
            self.sends += 1;
        } else {
            self.send_drops += 1;
        }
    }

    pub(super) fn handle_ack(&mut self, len: usize, addr: SocketAddr) {
        let b = &self.rx_buf[..len];
        let Some(ack) = parse_ack(b) else { return };
        let Some(peer) = self.peers.get_mut(&addr) else {
            self.acks_ignored += 1;
            return;
        };
        if !peer.bound {
            self.acks_ignored += 1;
            return;
        }
        let released = peer.reliable.ack(ack.seq, mono_ns() / 1_000_000) as u64;
        self.events_acked += released;
    }
}
