use std::io;
use std::time::Instant;

use engine_webrtc_protocol::{OP_ACK, OP_EVENT};

use crate::plane::mono_ns;
use crate::plane::udp::bucket::TokenBucket;
use crate::plane::udp::plane::{OP_INP, OP_JOIN, OP_PING, UdpPlane};

impl UdpPlane {
    pub fn on_readable(&mut self) {
        self.drain_pending();
        loop {
            let (len, addr) = match self.socket.borrow_mut().recv_from(&mut self.rx_buf) {
                Ok(r) => r,
                Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => break,
                Err(_) => {
                    self.recv_errors += 1;
                    break;
                }
            };
            let arrival_ns = mono_ns();
            self.pkts_in += 1;
            if len == 0 {
                continue;
            }
            let now = Instant::now();
            let allowed = match self.peers.get_mut(&addr) {
                Some(p) => p.bucket.allow(now),
                None => self
                    .strangers
                    .entry(addr)
                    .or_insert_with(|| TokenBucket::new_at(now))
                    .allow(now),
            };
            if !allowed {
                self.rate_limited += 1;
                continue;
            }
            if let Some(p) = self.peers.get_mut(&addr) {
                p.last_seen = now;
            }

            match self.rx_buf[0] {
                OP_JOIN => self.handle_join(len, addr, now),
                OP_INP => self.handle_inp(len, addr, arrival_ns),
                OP_PING => self.handle_ping(len, addr),
                OP_ACK => self.handle_ack(len, addr),
                OP_EVENT => {
                    self.events_in += 1;
                }
                _ => {}
            }
        }
    }
}
