use std::time::Instant;

use engine_bus::{PeerId, SimCommand};

use crate::plane::reliable::ReliableCounts;
use crate::plane::udp::plane::UdpPlane;

impl UdpPlane {
    pub fn reliable_counts(&self) -> ReliableCounts {
        let mut total = ReliableCounts::default();
        for peer in self.peers.values() {
            let c = peer.reliable.counts();
            total.queued += c.queued;
            total.sent += c.sent;
            total.resent += c.resent;
            total.acked += c.acked;
            total.expired += c.expired;
            total.rejected += c.rejected;
        }
        total
    }

    pub fn reliable_inflight(&self) -> usize {
        self.peers.values().map(|p| p.reliable.inflight()).sum()
    }

    pub fn reliable_refused(&self) -> u64 {
        self.reliable_refused
    }

    pub fn acks_ignored(&self) -> u64 {
        self.acks_ignored
    }

    pub fn events_acked(&self) -> u64 {
        self.events_acked
    }

    pub fn events_in(&self) -> u64 {
        self.events_in
    }

    pub fn peer_rtt_ms(&self, peer_id: &str) -> Option<f64> {
        let &addr = self.peer_addrs.get(peer_id)?;
        let peer = self.peers.get(&addr)?;
        peer.bound
            .then_some(peer.reliable.rtt_ms())
            .filter(|ms| *ms > 0.0)
    }

    pub fn sweep(&mut self) {
        self.drain_pending();
        let now = Instant::now();
        let peer_idle = self.peer_idle;
        let mut dropped: Vec<PeerId> = Vec::new();
        self.peers.retain(|_, peer| {
            if now.duration_since(peer.last_seen) > peer_idle {
                dropped.push(peer.session_id.clone());
                false
            } else {
                true
            }
        });
        for sid in dropped {
            self.peer_addrs.remove(&sid);
            let _ = self
                .bus
                .commands_tx
                .try_send(SimCommand::DropPeer { peer_id: sid });
        }
        self.strangers
            .retain(|_, bucket| now.duration_since(bucket.last) <= peer_idle);
    }

    pub fn peer_count(&self) -> usize {
        self.peers
            .values()
            .filter(|p| p.bound || p.pending_reply.is_some())
            .count()
    }

    pub fn stats(&self) -> (u64, u64, u64, u64, u64) {
        (
            self.pkts_in,
            self.denied,
            self.rate_limited,
            self.sends,
            self.send_drops,
        )
    }
}
