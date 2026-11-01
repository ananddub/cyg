use std::net::SocketAddr;
use std::time::Instant;

use crate::plane::udp::peer::Peer;
use crate::plane::udp::plane::{DENY_BAD_TOKEN, DENY_FULL, UdpPlane};

impl UdpPlane {
    pub(super) fn handle_join(&mut self, len: usize, addr: SocketAddr, now: Instant) {
        if len < 2 || len != 2 + self.rx_buf[1] as usize {
            return;
        }
        let token = self.rx_buf[2..len].to_vec();
        let Ok(token_str) = std::str::from_utf8(&token) else {
            self.send_deny(addr, DENY_BAD_TOKEN);
            return;
        };
        let grant = self.seats.verify(token_str);
        let Some(grant) = grant else {
            self.send_deny(addr, DENY_BAD_TOKEN);
            return;
        };
        if self.peers.contains_key(&addr) {
            return;
        }
        let (tx, rx) = crossbeam_channel::bounded::<i64>(1);
        let session_id = format!("udp-{}-{}", addr, self.session_counter);
        self.session_counter += 1;
        let cmd = engine_bus::SimCommand::BindPeer {
            peer_id: session_id.clone(),
            room: grant.room.clone(),
            reply: tx,
        };
        if self.bus.commands_tx.try_send(cmd).is_err() {
            self.send_deny(addr, DENY_FULL);
            return;
        }
        self.strangers.remove(&addr);
        self.peer_addrs.insert(session_id.clone(), addr);
        self.peers
            .insert(addr, Peer::new(session_id, now, grant.room, rx));
    }
}
