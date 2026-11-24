use std::net::SocketAddr;

use crossbeam_channel::TryRecvError;

use crate::plane::mono_ns;
use crate::plane::udp::plane::UdpPlane;

impl UdpPlane {
    pub(super) fn pump_peer(&mut self, addr: SocketAddr, now_ms: u64) {
        let socket = &self.socket;
        let peers = &mut self.peers;
        let Some(peer) = peers.get_mut(&addr) else {
            return;
        };
        let mut sent = 0u64;
        let mut drops = 0u64;
        peer.reliable.pump(now_ms, |wire| {
            let mut sock = socket.borrow_mut();
            match UdpPlane::send_raw(&mut sock, wire, addr) {
                Ok(()) => {
                    sent += 1;
                    true
                }
                Err(_) => {
                    drops += 1;
                    false
                }
            }
        });
        self.sends += sent;
        self.send_drops += drops;
    }

    pub fn pump_reliable(&mut self) {
        let now = mono_ns() / 1_000_000;
        let due: Vec<SocketAddr> = self
            .peers
            .iter()
            .filter(|(_, p)| p.bound && p.reliable.inflight() > 0)
            .map(|(addr, _)| *addr)
            .collect();
        for addr in due {
            self.pump_peer(addr, now);
        }
    }

    pub fn drain_pending(&mut self) {
        let mut rejected: Vec<SocketAddr> = Vec::new();
        for (&addr, peer) in self.peers.iter_mut() {
            let got = match peer.pending_reply.as_mut() {
                Some(rx) => match rx.try_recv() {
                    Ok(id) => Some(id),
                    Err(TryRecvError::Disconnected) => None,
                    Err(TryRecvError::Empty) => None,
                },
                None => None,
            };
            let Some(id) = got else { continue };
            peer.pending_reply = None;
            if id < 0 {
                let mut sock = self.socket.borrow_mut();
                UdpPlane::send_deny_raw(&mut sock, &mut self.scratch, &mut self.denied, addr, 4);
                drop(sock);
                rejected.push(addr);
                continue;
            }
            peer.player_id = id;
            peer.bound = true;
            crate::netcode::encode_bind(id, self.tick_hz, &peer.room, &mut self.scratch);
            let mut sock = self.socket.borrow_mut();
            match Self::send_raw(&mut sock, &self.scratch, addr) {
                Ok(()) => self.sends += 1,
                Err(_) => self.send_drops += 1,
            }
        }
        for addr in rejected {
            if let Some(peer) = self.peers.remove(&addr) {
                self.peer_addrs.remove(&peer.session_id);
            }
        }
    }
}
