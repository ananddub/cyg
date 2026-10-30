//! Reliable, ordered server→client event lane for the raw UDP plane.
//!
//! The plane's other outbound traffic is newest-wins: a delta the client never
//! saw is replaced by the next tick's delta, so nothing stores it. An **event**
//! (a hit, a kill, a respawn) has no successor — losing it loses the fact
//! forever. That is the one thing raw UDP must not do, and until this lane
//! existed `FrameKind::Reliable` was sent exactly once and forgotten.
//!
//! # Why Go-Back-N with a whole-window resend
//!
//! An event frame is [`EVENT_WIRE_LEN`] bytes, so a full window costs a few
//! hundred bytes per round. Selective retransmission (a NACK bitmap, like
//! lightyear's `last_ack_packet_id` + `u32` mask) buys bandwidth that is not the
//! constraint here and would put a second state machine between this plane and
//! the codec crate, which deliberately owns no retransmission at all. One
//! datagram per event, resend every unacked event in order, one cumulative ack:
//! the receiver-side rule fits in a compare.
//!
//! # Ordering
//!
//! Sequence numbers come from the wire, not from this lane: the caller stamps
//! `EventFrame.seq` from the room's `event_seq` (exactly as `host.rs` does for
//! the WebRTC lane), so one event carries one identity across both transports.
//! Acks are **cumulative**: `ack(n)` means "everything up to and including n is
//! delivered". The client-side rule is therefore: deliver `seq` only when it is
//! the next expected one, buffer the rest, and ack the highest contiguous
//! prefix. That is also why `pump` never lets a later event overtake an un-due
//! earlier one — under a cumulative ack the tail cannot be delivered until the
//! head is, so sending it early costs bandwidth and nothing else.
//!
//! # Timers
//!
//! The retransmit timeout is derived from a smoothed round-trip time, sampled
//! only from a **first** transmission's ack (Karn's rule: an ack that follows a
//! resend says nothing about which attempt it answers). Each unacked event
//! additionally backs off per attempt, so a peer that stops answering costs
//! bounded bandwidth rather than a resend storm, and `EVENT_LIFETIME` finally
//! retires it — a ten-second-old hit is not worth delivering at all.
//!
//! Time is monotonic milliseconds handed in by the caller (same epoch as
//! [`crate::plane::mono_ns`]), never `Instant`: the lane then stores no
//! process-lifetime anchor and its tests can drive the clock exactly.

use std::collections::VecDeque;
use std::fmt;

use engine_webrtc_protocol::{EVENT_WIRE_LEN, is_newer};

/// How many unacked events one peer may have outstanding. Above this the lane
/// refuses rather than queues: an event that cannot be resent is not reliable,
/// and the producer must decide what to do with the backpressure.
pub const MAX_INFLIGHT_EVENTS: usize = 32;

/// Floor and ceiling for the retransmit timeout, in ms. The floor keeps a LAN
/// peer from resending on every poll pass; the ceiling keeps a stalled peer's
/// window from freezing the lane for minutes.
pub const MIN_RTO: u64 = 40;
pub const MAX_RTO: u64 = 2_000;

/// Before the first ack there is no measurement, so start at a round trip a
/// 128 Hz poll loop can actually service.
pub const INITIAL_RTO: u64 = 150;

/// Per-attempt doubling cap. Eight attempts at a 2 s ceiling is tens of seconds
/// of retries, which outlives every event worth delivering.
pub const MAX_BACKOFF_SHIFT: u32 = 3;

/// An event nobody has acknowledged by now (ms) will not change the game state
/// a client still holds.
pub const EVENT_LIFETIME: u64 = 10_000;

/// The reliable lane carries exactly one event frame per datagram: an event is
/// fixed-size, so a resend is a single `send_to` and a lost datagram loses
/// precisely one event.
/// Historical cap: the plane's event frames are exactly this long. The lane
/// itself accepts ANY payload size now — this constant stays for the plane's
/// own frame assertions and metrics only.
pub const MAX_EVENT_BYTES: usize = EVENT_WIRE_LEN;

/// Largest single reliable payload the lane will buffer (2 KiB — comfortably
/// above a datagram and small enough that 32 inflight fit any rcvbuf).
pub const MAX_RELIABLE_BYTES: usize = 2048;

/// The lane refused an event. The counters behind each variant are what an
/// operator reads to tell a flood from a stalled client.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QueueError {
    /// `MAX_INFLIGHT_EVENTS` are already waiting for an ack.
    WindowFull { inflight: usize },
    /// Payload above `MAX_RELIABLE_BYTES` — the producer must chunk.
    TooBig { bytes: usize },
    /// `seq` is not newer than the last seq this lane queued.
    NotNewer { seq: u16, last: u16 },
}

impl fmt::Display for QueueError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            QueueError::WindowFull { inflight } => write!(
                f,
                "reliable window is full ({inflight}/{MAX_INFLIGHT_EVENTS} events await an ack)"
            ),
            QueueError::TooBig { bytes } => write!(
                f,
                "a reliable payload exceeds {MAX_RELIABLE_BYTES} bytes, got {bytes}"
            ),
            QueueError::NotNewer { seq, last } => {
                write!(f, "event seq {seq} is not newer than the queued {last}")
            }
        }
    }
}

impl std::error::Error for QueueError {}

#[derive(Debug)]
struct Pending {
    seq: u16,
    wire: Vec<u8>,
    /// When this event entered the lane: `EVENT_LIFETIME` is measured from here.
    since: u64,
    /// When the last transmission went out. The RTT sample is taken from this,
    /// and only when `attempts == 0`.
    last_tx: u64,
    /// The next instant a resend is due.
    due: u64,
    /// Resends already performed for this event.
    attempts: u32,
    /// False until the first transmission has actually left the socket, so a
    /// `WouldBlock` retry is not counted as a retransmission.
    sent_once: bool,
}

/// Everything the lane counted, for the plane's telemetry.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ReliableCounts {
    /// Events handed to the lane.
    pub queued: u64,
    /// First transmissions.
    pub sent: u64,
    /// Retransmitted datagrams.
    pub resent: u64,
    /// Events released by a cumulative ack.
    pub acked: u64,
    /// Events retired without ever being acked.
    pub expired: u64,
    /// `queue` refusals (window full, bad frame, stale seq).
    pub rejected: u64,
}

/// One peer's unacked events, in sequence order, oldest at the front.
#[derive(Debug)]
pub struct ReliableLane {
    window: VecDeque<Pending>,
    last_seq: Option<u16>,
    /// Smoothed round trip, in milliseconds, from first-transmission acks.
    srtt_ms: f64,
    rto: u64,
    counts: ReliableCounts,
}

impl Default for ReliableLane {
    fn default() -> Self {
        Self::new()
    }
}

impl ReliableLane {
    pub fn new() -> ReliableLane {
        ReliableLane {
            window: VecDeque::new(),
            last_seq: None,
            srtt_ms: 0.0,
            rto: INITIAL_RTO,
            counts: ReliableCounts::default(),
        }
    }

    pub fn counts(&self) -> ReliableCounts {
        self.counts
    }

    /// Events waiting for an ack right now.
    pub fn inflight(&self) -> usize {
        self.window.len()
    }

    /// Smoothed round-trip estimate in ms; `0.0` until the first ack lands.
    pub fn rtt_ms(&self) -> f64 {
        self.srtt_ms
    }

    /// The current retransmit timeout in ms, after the RTT estimate and the clamp.
    pub fn rto(&self) -> u64 {
        self.rto
    }

    /// The next instant (monotonic ms) any unacked event is due to leave the
    /// socket, or `None` when the window is empty. The plane pumps at this
    /// deadline so a resend never runs late behind the loop's own cadence.
    pub fn next_due(&self) -> Option<u64> {
        self.window.front().map(|entry| entry.due)
    }

    /// Queue one framed event for ordered, acknowledged delivery. `seq` must be
    /// the seq already written into `wire`, so an ack names the same number the
    /// client read out of the frame. `now_ms` is a monotonic millisecond stamp
    /// (`mono_ns() / 1_000_000`).
    pub fn queue(&mut self, seq: u16, wire: Vec<u8>, now_ms: u64) -> Result<(), QueueError> {
        if wire.is_empty() || wire.len() > MAX_RELIABLE_BYTES {
            self.counts.rejected += 1;
            return Err(QueueError::TooBig { bytes: wire.len() });
        }
        if let Some(last) = self.last_seq
            && !is_newer(last, seq)
        {
            self.counts.rejected += 1;
            return Err(QueueError::NotNewer { seq, last });
        }
        if self.window.len() >= MAX_INFLIGHT_EVENTS {
            self.counts.rejected += 1;
            return Err(QueueError::WindowFull {
                inflight: self.window.len(),
            });
        }
        self.last_seq = Some(seq);
        self.counts.queued += 1;
        // `due = now` so the next `pump` performs the first transmission; the
        // plane pumps right after queueing, so an event never waits for a tick.
        self.window.push_back(Pending {
            seq,
            wire,
            since: now_ms,
            last_tx: now_ms,
            due: now_ms,
            attempts: 0,
            sent_once: false,
        });
        Ok(())
    }

    /// Send every event whose timer has expired, oldest first, and retire the
    /// ones that outlived `EVENT_LIFETIME`. `send` returns false when the
    /// datagram could not go out; the event stays due and is tried again.
    pub fn pump(&mut self, now_ms: u64, mut send: impl FnMut(&[u8]) -> bool) {
        self.retire_expired(now_ms);
        // Read the timeout once: the window is mutably borrowed by the iterator
        // below, and a resend schedule must not appear to touch `self`.
        let rto = self.rto;
        for entry in self.window.iter_mut() {
            if entry.due > now_ms {
                break;
            }
            if !send(&entry.wire) {
                break;
            }
            entry.last_tx = now_ms;
            if entry.sent_once {
                entry.attempts += 1;
                self.counts.resent += 1;
            } else {
                entry.sent_once = true;
                self.counts.sent += 1;
            }
            entry.due = now_ms + Self::backoff(rto, entry.attempts);
        }
    }

    /// A cumulative ack for `seq`: release every event at or behind it. Returns
    /// how many events the ack released; `0` means it named something this lane
    /// is not waiting for (a duplicate, or a seq from a peer that never got it),
    /// which is counted rather than fatal.
    pub fn ack(&mut self, seq: u16, now_ms: u64) -> usize {
        self.ack_selective(seq, &[], now_ms)
    }

    /// QUIC-style selective ack (SACK): release everything at or behind `cum`
    /// AND every event inside `ranges` (disjoint pairs, above cum). Only
    /// genuinely-missing frames stay in the window — already-delivered frames
    /// above a gap are never retransmitted.
    pub fn ack_selective(&mut self, cum: u16, ranges: &[(u16, u16)], now_ms: u64) -> usize {
        let covered = |seq: u16| -> bool {
            // is_newer(last, tick): "tick is newer than last". seq at-or-behind
            // cum == NOT (seq newer than cum) == !is_newer(cum, seq).
            if !is_newer(cum, seq) {
                return true;
            }
            ranges.iter().any(|&(s, e)| {
                let off = seq.wrapping_sub(s);
                let len = e.wrapping_sub(s);
                off <= len
            })
        };
        let mut released: usize = 0;
        let mut rtt_samples: Vec<u64> = Vec::new();
        self.window.retain(|pending| {
            if covered(pending.seq) {
                released += 1;
                // Karn's rule: only first-transmission releases measure RTT.
                if pending.attempts == 0 {
                    rtt_samples.push(pending.last_tx);
                }
                false
            } else {
                true
            }
        });
        for last_tx in rtt_samples {
            let sample = now_ms.saturating_sub(last_tx) as f64;
            if sample > 0.0 {
                if self.srtt_ms == 0.0 {
                    self.srtt_ms = sample;
                } else {
                    self.srtt_ms = self.srtt_ms * 0.875 + sample * 0.125;
                }
                self.rto = Self::rto_from(self.srtt_ms);
            }
        }
        self.counts.acked += released as u64;
        released
    }

    /// Drop the oldest events whose lifetime expired. Only ever from the front:
    /// an expired head is what unblocks the ordered window behind it.
    fn retire_expired(&mut self, now_ms: u64) {
        while let Some(front) = self.window.front() {
            if now_ms.saturating_sub(front.since) < EVENT_LIFETIME {
                break;
            }
            self.window.pop_front();
            self.counts.expired += 1;
        }
    }

    fn backoff(rto: u64, attempts: u32) -> u64 {
        rto.saturating_mul(1u64 << attempts.min(MAX_BACKOFF_SHIFT))
    }

    fn rto_from(srtt_ms: f64) -> u64 {
        // 2x the smoothed RTT is the classic margin. Clamped at both ends: a
        // 3 ms LAN peer still waits long enough for an ack to come back, and a
        // 5 s peer does not make the lane resend faster than it can be answered.
        (srtt_ms * 2.0).clamp(MIN_RTO as f64, MAX_RTO as f64) as u64
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use engine_webrtc_protocol::{EventFrame, EventKind, OP_EVENT, encode_event};

    /// A real event frame, so the seq the lane tracks is the seq a client would
    /// read out of the wire.
    fn wire(seq: u16) -> Vec<u8> {
        let mut out = Vec::new();
        encode_event(
            &EventFrame {
                seq,
                kind: EventKind::Kill,
                from: 3,
                target: u16::MAX,
                tick: 77,
                value: 1.0,
            },
            &mut out,
        );
        assert_eq!(out.len(), MAX_EVENT_BYTES);
        assert_eq!(out[0], OP_EVENT);
        out
    }

    /// Records what the lane put on the wire, and can pretend the socket was
    /// full.
    #[derive(Default)]
    struct Sink {
        sent: Vec<u16>,
        accept: bool,
    }

    impl Sink {
        fn take(&mut self, bytes: &[u8]) -> bool {
            if !self.accept {
                return false;
            }
            self.sent
                .push(EventFrame::decode(bytes).expect("event wire").seq);
            true
        }
    }

    #[test]
    fn a_queued_event_goes_out_once_and_is_not_resent_before_the_timeout() {
        let mut lane = ReliableLane::new();
        let mut sink = Sink {
            accept: true,
            ..Sink::default()
        };

        lane.queue(7, wire(7), 0).unwrap();
        lane.pump(0, |b| sink.take(b));
        assert_eq!(sink.sent, vec![7], "queue + pump = exactly one datagram");
        assert_eq!(lane.counts().sent, 1);
        assert_eq!(lane.counts().resent, 0);

        lane.pump(lane.rto() / 2, |b| sink.take(b));
        assert_eq!(
            sink.sent.len(),
            1,
            "no resend while the ack is still owed time"
        );
    }

    #[test]
    fn an_unacked_event_is_retransmitted_with_a_bounded_backoff() {
        let mut lane = ReliableLane::new();
        let mut sink = Sink {
            accept: true,
            ..Sink::default()
        };
        lane.queue(1, wire(1), 0).unwrap();

        let rto = lane.rto();
        lane.pump(0, |b| sink.take(b));
        let mut clock = 0;
        // Pump exactly on each scheduled deadline: first rto, then the doubled
        // intervals until the per-attempt cap, then capped steps.
        for round in 0..7 {
            clock += rto << round.min(MAX_BACKOFF_SHIFT);
            lane.pump(clock, |b| sink.take(b));
        }
        assert_eq!(
            sink.sent,
            vec![1; 8],
            "one datagram per deadline (the first is the send itself)"
        );
        assert_eq!(lane.counts().sent, 1);
        assert_eq!(lane.counts().resent, 7);

        // The window is empty afterwards, so nothing goes out on later passes.
        sink.sent.clear();
        lane.pump(clock + rto, |b| sink.take(b));
        assert!(sink.sent.is_empty());

        // The doubling is capped, so a deaf peer cannot grow the delay forever.
        assert_eq!(ReliableLane::backoff(rto, 0), rto);
        assert_eq!(ReliableLane::backoff(rto, 99), rto << MAX_BACKOFF_SHIFT);
    }

    #[test]
    fn the_window_is_strictly_ordered_behind_an_un_due_head() {
        let mut lane = ReliableLane::new();
        let mut sink = Sink {
            accept: true,
            ..Sink::default()
        };
        lane.queue(1, wire(1), 0).unwrap();
        lane.pump(0, |b| sink.take(b));
        assert_eq!(sink.sent, vec![1]);

        // 2 arrives while the head is still backed off: under a cumulative ack
        // the tail cannot be delivered before the head is resolved, so nothing
        // leaves the socket — the pump stops at the first not-due entry.
        lane.queue(2, wire(2), 5).unwrap();
        lane.pump(5, |b| sink.take(b));
        assert_eq!(sink.sent, vec![1], "a backed-off head holds the tail back");

        // When the head is due again, the pump resends it and the tail's own
        // deadline has passed too, so both leave in one pass — the head's
        // resend first, then 2, so wire order is still queue order.
        let head_due = lane.next_due().expect("two events outstanding");
        assert_eq!(head_due, lane.rto());
        lane.pump(head_due, |b| sink.take(b));
        assert_eq!(
            sink.sent,
            vec![1, 1, 2],
            "the head resends, then 2 goes in order"
        );
    }

    #[test]
    fn a_cumulative_ack_releases_the_prefix_and_nothing_is_resent() {
        let mut lane = ReliableLane::new();
        let mut sink = Sink {
            accept: true,
            ..Sink::default()
        };
        for seq in 1..=5 {
            lane.queue(seq, wire(seq), 0).unwrap();
        }
        // Every queued event was due at 0, so one pass puts the whole window on
        // the wire, oldest first — one datagram each, in queue order.
        lane.pump(0, |b| sink.take(b));
        assert_eq!(sink.sent, vec![1, 2, 3, 4, 5]);

        assert_eq!(lane.ack(4, 60), 4, "ack(4) releases 1..=4");
        assert_eq!(lane.inflight(), 1);
        assert_eq!(lane.counts().acked, 4);

        sink.sent.clear();
        lane.pump(lane.next_due().unwrap() - 1, |b| sink.take(b));
        assert!(
            sink.sent.is_empty(),
            "the new head is not due yet, so 5 stays quiet"
        );
        lane.pump(lane.next_due().unwrap(), |b| sink.take(b));
        assert_eq!(sink.sent, vec![5], "only the still-unacked event is resent");
    }

    #[test]
    fn a_duplicate_or_unrelated_ack_releases_nothing_and_moves_no_estimate() {
        let mut lane = ReliableLane::new();
        lane.queue(3, wire(3), 0).unwrap();
        lane.pump(0, |_| true);
        assert_eq!(lane.ack(3, 50), 1);
        let rtt = lane.rtt_ms();
        assert!(rtt > 0.0, "the first ack must produce a sample");

        assert_eq!(lane.ack(3, 900), 0, "duplicate");
        assert_eq!(lane.ack(9, 950), 0, "window is empty");
        assert_eq!(lane.rtt_ms(), rtt, "a re-delivered ack is not a round trip");
    }

    #[test]
    fn an_ack_of_a_retransmitted_event_does_not_measure_the_round_trip() {
        let mut lane = ReliableLane::new();
        let mut sink = Sink {
            accept: true,
            ..Sink::default()
        };
        lane.queue(1, wire(1), 0).unwrap();
        lane.pump(0, |b| sink.take(b));
        let resent = lane.rto();
        lane.pump(resent, |b| sink.take(b));
        assert_eq!(lane.counts().resent, 1);

        // This ack could be answering either transmission, so it must not move
        // the estimate at all (Karn's rule).
        assert_eq!(lane.ack(1, resent + 5), 1);
        assert_eq!(lane.rtt_ms(), 0.0);
        assert_eq!(lane.rto(), INITIAL_RTO);
    }

    #[test]
    fn a_slow_ack_pulls_the_timeout_toward_the_measured_round_trip() {
        let mut lane = ReliableLane::new();
        for seq in 1..=3 {
            lane.queue(seq, wire(seq), 0).unwrap();
        }
        // One pass transmits all three (all were due at 0), each recording its
        // own send time, so the ack samples below are the send→ack round trips.
        lane.pump(0, |_| true);

        lane.ack(1, 500);
        // 500 ms round trip → 1000 ms, under the 2 s ceiling.
        assert_eq!(lane.rto(), 1000);

        lane.ack(2, 800);
        // A second sample smooths rather than snaps: 0.875·500 + 0.125·800.
        assert_eq!(lane.rtt_ms(), 537.5);
        assert_eq!(lane.rto(), 1075, "the timeout followed the estimate");

        // And the floor still applies to an absurdly fast peer.
        assert_eq!(
            ReliableLane::rto_from(1.0),
            MIN_RTO,
            "a 1 ms sample cannot produce a sub-floor timeout"
        );
        assert_eq!(ReliableLane::rto_from(90_000.0), MAX_RTO);
    }

    #[test]
    fn an_event_nobody_acks_is_retired_after_its_lifetime() {
        let mut lane = ReliableLane::new();
        lane.queue(1, wire(1), 0).unwrap();
        lane.queue(2, wire(2), 0).unwrap();
        lane.pump(0, |_| true);

        // Dead-on at the lifetime boundary: both go, oldest first, and the lane
        // stops sending afterwards rather than resending into the void.
        let mut sink = Sink {
            accept: true,
            ..Sink::default()
        };
        lane.pump(EVENT_LIFETIME, |b| sink.take(b));
        assert_eq!(lane.inflight(), 0);
        assert_eq!(lane.counts().expired, 2);
        assert!(
            sink.sent.is_empty(),
            "an expired event is never transmitted"
        );

        // The window is free again, and a fresh seq still queues (last_seq is
        // kept so a late old seq cannot re-enter).
        assert!(lane.queue(3, wire(3), EVENT_LIFETIME).is_ok());
        assert_eq!(lane.ack(3, EVENT_LIFETIME + 20), 1);
        assert!(matches!(
            lane.queue(1, wire(1), EVENT_LIFETIME),
            Err(QueueError::NotNewer { seq: 1, last: 3 })
        ));
    }

    #[test]
    fn the_window_fills_and_refuses_instead_of_growing_without_bound() {
        let mut lane = ReliableLane::new();
        for seq in 0..MAX_INFLIGHT_EVENTS as u16 {
            lane.queue(seq, wire(seq), 0).expect("fills the window");
        }
        assert_eq!(lane.inflight(), MAX_INFLIGHT_EVENTS);
        assert_eq!(
            lane.queue(
                MAX_INFLIGHT_EVENTS as u16,
                wire(MAX_INFLIGHT_EVENTS as u16),
                0
            ),
            Err(QueueError::WindowFull {
                inflight: MAX_INFLIGHT_EVENTS
            })
        );
        assert_eq!(lane.counts().rejected, 1);
        assert_eq!(
            lane.counts().queued,
            MAX_INFLIGHT_EVENTS as u64,
            "a refusal does not queue"
        );

        // Freeing the head makes room: the producer keeps working after one ack.
        lane.pump(0, |_| true);
        assert_eq!(lane.ack(0, 10), 1);
        assert!(
            lane.queue(
                MAX_INFLIGHT_EVENTS as u16,
                wire(MAX_INFLIGHT_EVENTS as u16),
                10
            )
            .is_ok()
        );
    }

    #[test]
    fn arbitrary_payload_sizes_are_reliable_and_oversize_is_refused() {
        let mut lane = ReliableLane::new();
        // Any size 1..=MAX_RELIABLE_BYTES is a legal payload now — event
        // frames, chat, JSON blobs, chunked files.
        let mut seq = 1u16;
        for size in [1usize, 9, 14, 64, 512, 1024, MAX_RELIABLE_BYTES] {
            assert!(
                lane.queue(seq, vec![7u8; size], 0).is_ok(),
                "size {size} must queue"
            );
            seq = seq.wrapping_add(1);
        }
        // Empty and oversize are producer bugs, refused with the count.
        for bad in [vec![0u8; 0], vec![0u8; MAX_RELIABLE_BYTES + 1]] {
            let len = bad.len();
            assert_eq!(
                lane.queue(seq, bad, 0),
                Err(QueueError::TooBig { bytes: len })
            );
        }
        assert_eq!(lane.counts().rejected, 2);
        assert_eq!(lane.inflight(), 7);
    }

    #[test]
    fn a_refused_send_leaves_the_event_due_so_the_next_pass_retries_it() {
        let mut lane = ReliableLane::new();
        let mut sink = Sink {
            accept: false,
            ..Sink::default()
        };
        lane.queue(1, wire(1), 0).unwrap();
        lane.pump(0, |b| sink.take(b));
        assert_eq!(
            lane.counts().sent,
            0,
            "a datagram the socket refused is not a send"
        );
        assert_eq!(lane.window[0].attempts, 0, "and it costs no backoff");

        sink.accept = true;
        lane.pump(0, |b| sink.take(b));
        assert_eq!(
            sink.sent,
            vec![1],
            "the next pass transmits it, still due immediately"
        );
        assert_eq!(lane.counts().sent, 1);
        assert_eq!(lane.counts().resent, 0);
    }

    #[test]
    fn sequence_wrap_does_not_fool_the_order_or_the_ack() {
        let mut lane = ReliableLane::new();
        let mut sink = Sink {
            accept: true,
            ..Sink::default()
        };
        // 65534 → 65535 → 0 → 1, across the u16 wrap. One pass transmits the
        // whole window, in order.
        for seq in [65534u16, 65535, 0, 1] {
            lane.queue(seq, wire(seq), 0).unwrap();
        }
        lane.pump(0, |b| sink.take(b));
        assert_eq!(sink.sent, vec![65534, 65535, 0, 1]);
        // A cumulative ack of 0 releases the first three: they are all "behind"
        // it in wrap-safe order.
        assert_eq!(lane.ack(0, 30), 3);
        assert_eq!(lane.inflight(), 1);
        // And the lane refuses to queue something older than what it last saw.
        assert!(matches!(
            lane.queue(65534, wire(65534), 30),
            Err(QueueError::NotNewer { .. })
        ));
    }
}
