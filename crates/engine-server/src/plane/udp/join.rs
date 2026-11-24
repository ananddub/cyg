use std::net::SocketAddr;
use std::time::Instant;

use crate::netcode::admission::{self, DenyReason, Lane};
use crate::plane::udp::peer::Peer;
use crate::plane::udp::plane::UdpPlane;

impl UdpPlane {
    pub(super) fn handle_join(&mut self, len: usize, addr: SocketAddr, now: Instant) {
        let admitted = match admission::admit_join_frame(
            Lane::UdpPlane,
            &self.seats,
            &self.rx_buf[..len],
            Some(&self.room_name),
        ) {
            Ok(a) => a,
            Err(reason) => {
                self.send_deny(addr, reason.code());
                return;
            }
        };
        if self.peers.contains_key(&addr) {
            return;
        }
        let session_id = format!("udp-{}-{}", addr, self.session_counter);
        self.session_counter = self.session_counter.wrapping_add(1);
        let (tx, rx) = crossbeam_channel::bounded::<i64>(1);
        let cmd = engine_bus::SimCommand::BindPeer {
            peer_id: session_id.clone(),
            room: admitted.room.clone(),
            reply: tx,
        };
        if self.bus.commands_tx.try_send(cmd).is_err() {
            self.send_deny(addr, DenyReason::RoomFull.code());
            return;
        }
        self.strangers.remove(&addr);
        self.peer_addrs.insert(session_id.clone(), addr);
        self.peers
            .insert(addr, Peer::new(session_id, now, admitted.room, rx));
    }
}
