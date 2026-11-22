//! Deterministic 150 ms RTT / 5% loss soak.
//!
//! This runs the real input-batch parser, the reliable event lane (PTO, pacing,
//! congestion window) and the rewind track through a two-way lossy link. It is
//! deterministic so CI can assert convergence instead of waiting for a flaky
//! live network.

use std::collections::{HashSet, VecDeque};

use engine_webrtc_protocol::is_newer;

use crate::netcode::{MAX_INPUT_BATCH_FRAMES, parse_input_batch};
use crate::plane::reliable::ReliableLane;
use crate::rooms::RewindTrack;

const ONE_WAY_MS: u64 = 75;
const TICK_MS: u64 = 33;
const SOAK_MS: u64 = 10_000;
const DRAIN_MS: u64 = 1_000;
const LOSS_EVERY: u64 = 20;

#[derive(Default)]
struct Link {
    c2s: VecDeque<(u64, Vec<u8>)>,
    s2c: VecDeque<(u64, Vec<u8>)>,
    c2s_seq: u64,
    s2c_seq: u64,
    dropped: u64,
}

impl Link {
    fn send_c2s(&mut self, now: u64, payload: Vec<u8>) {
        self.c2s_seq += 1;
        if self.c2s_seq % LOSS_EVERY == 0 {
            self.dropped += 1;
        } else {
            self.c2s.push_back((now + ONE_WAY_MS, payload));
        }
    }

    fn send_s2c(&mut self, now: u64, payload: Vec<u8>) {
        self.s2c_seq += 1;
        if self.s2c_seq % LOSS_EVERY == 0 {
            self.dropped += 1;
        } else {
            self.s2c.push_back((now + ONE_WAY_MS, payload));
        }
    }

    fn due(queue: &mut VecDeque<(u64, Vec<u8>)>, now: u64) -> Vec<Vec<u8>> {
        let mut out = Vec::new();
        while let Some((at, _)) = queue.front() {
            if *at > now {
                break;
            }
            out.push(queue.pop_front().unwrap().1);
        }
        out
    }

    fn c2s_due(&mut self, now: u64) -> Vec<Vec<u8>> {
        Self::due(&mut self.c2s, now)
    }

    fn s2c_due(&mut self, now: u64) -> Vec<Vec<u8>> {
        Self::due(&mut self.s2c, now)
    }
}

fn batch(frames: &[(u16, &[u8])]) -> Vec<u8> {
    let mut out = vec![0x05, 13];
    out.extend_from_slice(b"__input_batch");
    out.push(frames.len() as u8);
    for (seq, frame) in frames {
        out.extend_from_slice(&seq.to_le_bytes());
        out.extend_from_slice(&(frame.len() as u16).to_le_bytes());
        out.extend_from_slice(frame);
    }
    out
}

#[test]
fn simulated_150ms_rtt_and_5pct_loss_converges() {
    let mut link = Link::default();
    let mut server_lane = ReliableLane::new();
    let mut rewind = RewindTrack::default();
    let mut client_seq: u16 = 0;
    let mut history: Vec<(u16, Vec<u8>)> = Vec::new();
    let mut acked: i32 = -1;
    let mut server_last: Option<u16> = None;
    let mut applied = 0u64;
    let mut seen: HashSet<u16> = HashSet::new();
    let mut sent = 0u64;
    let mut input_count = 0u64;
    let mut server_ack_seq: u16 = 0;

    let end = SOAK_MS + DRAIN_MS;
    for now in (0..=end).step_by(1) {
        // Client tick. After the input phase it keeps resending the last
        // unacked batch, which is exactly what the browser client does.
        if now % TICK_MS == 0 && (now <= SOAK_MS || !history.is_empty()) {
            if now <= SOAK_MS {
                client_seq = client_seq.wrapping_add(1);
                let frame = [client_seq as u8, (now / TICK_MS) as u8];
                history.push((client_seq, frame.to_vec()));
                while history.len() > MAX_INPUT_BATCH_FRAMES {
                    history.remove(0);
                }
                input_count += 1;
            }
            // Keep the replay ring bounded to three frames; a 5% loss link
            // should still recover because every packet carries the last 3.
            let replay: Vec<(u16, &[u8])> = history
                .iter()
                .rev()
                .take(3)
                .map(|(seq, frame)| (*seq, frame.as_slice()))
                .collect();
            if !replay.is_empty() {
                let wire = batch(&replay);
                link.send_c2s(now, wire);
                sent += 1;
            }
        }

        // Server receives client packets.
        for wire in link.c2s_due(now) {
            if wire.first() == Some(&0xAC) && wire.len() == 3 {
                let _ = server_lane.ack(u16::from_le_bytes([wire[1], wire[2]]), now);
                continue;
            }
            let Some(parsed) = parse_input_batch(&wire) else {
                continue;
            };
            for (seq, _frame) in &parsed.frames {
                if seen.insert(*seq) {
                    applied += 1;
                }
                if server_last.is_none() || is_newer(server_last.unwrap(), *seq) {
                    server_last = Some(*seq);
                }
            }
            if let Some(seq) = server_last
                && (server_ack_seq == 0 || is_newer(server_ack_seq, seq))
            {
                server_ack_seq = seq;
                let _ = server_lane.queue(seq, seq.to_le_bytes().to_vec(), now);
            }
            let pos = applied as f64;
            let _ = rewind.record_time(now as i64 / 1000, now as f64, vec![pos]);
        }

        // Reliable lane pumps its ack events into the lossy link.
        let mut out = Vec::new();
        server_lane.pump(now, |wire| {
            out.push(wire.to_vec());
            true
        });
        for wire in out {
            link.send_s2c(now, wire);
        }

        // Client receives ack events and returns an ack packet.
        for wire in link.s2c_due(now) {
            if wire.len() < 2 {
                continue;
            }
            let seq = u16::from_le_bytes([wire[0], wire[1]]);
            if acked < 0 || is_newer(acked as u16, seq) {
                acked = seq as i32;
                history.retain(|(s, _)| {
                    let age = (seq.wrapping_sub(*s)) & 0xffff;
                    age >= 32768
                });
            }
            link.send_c2s(now, vec![0xAC, wire[0], wire[1]]);
        }

    }

    let counts = server_lane.counts();
    assert!(input_count > 250, "soak actually sent inputs: {input_count}");
    assert_eq!(
        server_last,
        Some(client_seq),
        "server saw the newest client input"
    );
    assert_eq!(history.len(), 0, "client replay ring drained after ack");
    assert_eq!(server_lane.inflight(), 0, "reliable acks all released");
    assert!(counts.acked > 0, "acks were delivered, not just dropped");
    assert!(counts.loss_events > 0, "the link actually lost packets");
    assert!(counts.resent > 0, "PTO retransmitted on loss");
    assert_eq!(
        applied, input_count,
        "every client frame was applied exactly once ({})",
        applied
    );

    let historical = rewind
        .at_time(end as f64 - 150.0)
        .expect("rewind window has a sample");
    assert_eq!(historical.len(), 1);
    assert!(historical[0] >= 0.0 && historical[0] <= applied as f64);
}
