//! The host-facing send surface: stamped deltas on the unreliable lane and
//! already-framed messages on either lane.

use super::WebRtcServer;
use super::contract::{DeltaTooLarge, Lane, split_delta};
use super::session::{Sink, WebRtcSession};
use super::types::{DeltaSendReport, WriteAttempt};

impl WebRtcServer {
    /// Stamp, fragment and send one world delta on `game-unreliable`.
    ///
    /// The caller owns `seq` — the room-wide delta counter, identical for every
    /// recipient of one tick — and `tick`. A body that needs more than
    /// [`engine_webrtc_protocol::MAX_FRAGMENTS`] chunks is refused as a whole and
    /// counted: never truncated, because a partial delta is a corrupt world state
    /// and an oversized frame is a message SCTP loses entirely.
    ///
    /// Frames SCTP will not take are queued for this session under the
    /// newest-wins rule, so the poll loop never blocks on one slow client.
    pub fn send_delta(
        &mut self,
        client_id: &str,
        seq: u16,
        tick: u32,
        body: &[u8],
    ) -> Result<DeltaSendReport, DeltaTooLarge> {
        let mut frames = Vec::new();
        let fragments = match split_delta(seq, tick, body, &mut frames) {
            Ok(fragments) => fragments,
            Err(err) => {
                self.sink.stats.deltas_refused_oversized += 1;
                return Err(err);
            }
        };
        let Some(session) = self.sessions.get_mut(client_id) else {
            self.sink.stats.frames_dropped_no_channel += fragments as u64;
            return Ok(DeltaSendReport::skipped(fragments));
        };
        Ok(Self::send_delta_frames(
            session,
            seq,
            tick,
            frames,
            fragments,
            &mut self.sink,
        ))
    }

    /// Send one stamped delta to every session with an open unreliable lane.
    /// Returns the number of sessions that received all of its frames.
    ///
    /// `seq` is room-wide, so the frames are identical for every recipient and
    /// the split is done once.
    pub fn broadcast_delta(&mut self, seq: u16, tick: u32, body: &[u8]) -> usize {
        let mut prototype = Vec::new();
        let fragments = match split_delta(seq, tick, body, &mut prototype) {
            Ok(fragments) => fragments,
            Err(_) => {
                self.sink.stats.deltas_refused_oversized += 1;
                return 0;
            }
        };
        let mut served = 0;
        for session in self.sessions.values_mut() {
            let report = Self::send_delta_frames(
                session,
                seq,
                tick,
                prototype.clone(),
                fragments,
                &mut self.sink,
            );
            if report.delivered() {
                served += 1;
            }
        }
        served
    }

    /// Send one already-framed message on the unreliable lane (`game-unreliable`).
    ///
    /// This is the raw lane primitive; prefer [`Self::send_delta`] for world
    /// state, which stamps and fragments for you. A message bigger than the
    /// path MTU is refused and counted rather than truncated or handed to SCTP
    /// for its own reassembly.
    pub fn send_unreliable(&mut self, client_id: &str, data: &[u8]) -> bool {
        self.send_frame(Lane::Unreliable, client_id, data)
    }

    /// Send one already-framed message on the reliable lane (`game-reliable`).
    pub fn send_reliable(&mut self, client_id: &str, data: &[u8]) -> bool {
        self.send_frame(Lane::Reliable, client_id, data)
    }

    /// Broadcast data to all clients with an open `game-unreliable` channel.
    pub fn broadcast_unreliable(&mut self, data: &[u8]) -> usize {
        self.broadcast_frame(Lane::Unreliable, data)
    }

    /// Broadcast data to all clients with an open `game-reliable` channel.
    pub fn broadcast_reliable(&mut self, data: &[u8]) -> usize {
        self.broadcast_frame(Lane::Reliable, data)
    }

    /// Send a whole split delta: write what the lane will take, queue the rest.
    pub(super) fn send_delta_frames(
        session: &mut WebRtcSession,
        seq: u16,
        tick: u32,
        frames: Vec<Vec<u8>>,
        fragments: usize,
        sink: &mut Sink,
    ) -> DeltaSendReport {
        let lane = Lane::Unreliable;
        let Some(cid) = session.channel_id(lane) else {
            sink.stats.frames_dropped_no_channel += frames.len() as u64;
            return DeltaSendReport::skipped(fragments);
        };

        // A queued tick nobody wants any more is worse than an empty queue: it delays the newest delta behind world state the client discards on arrival.
        let superseded = if session.queue(lane).is_empty() {
            0
        } else {
            session.queue(lane).purge_older_than(seq)
        };
        if superseded > 0 {
            sink.stats.deltas_superseded += superseded as u64;
        }

        let mut accepted = 0usize;
        let mut dropped = 0usize;
        // Frames already waiting means this delta joins the queue too, so a partially delivered delta cannot interleave with the one before it.
        let mut hold_from: Option<usize> = if session.queue(lane).is_empty() {
            None
        } else {
            Some(0)
        };

        if hold_from.is_none() {
            for (idx, frame) in frames.iter().enumerate() {
                match Self::try_write(session, cid, frame) {
                    WriteAttempt::Accepted => {
                        sink.stats.frames_sent += 1;
                        accepted += 1;
                    }
                    WriteAttempt::Saturated => {
                        hold_from = Some(idx);
                        break;
                    }
                    WriteAttempt::Lost => {
                        sink.stats.write_errors += 1;
                        dropped = frames.len() - idx;
                        sink.stats.frames_dropped_no_channel += dropped as u64;
                        break;
                    }
                }
            }
        }

        if let Some(start) = hold_from {
            let tail = &frames[start..];
            let held = Self::enqueue(session, lane, Some(seq), Some(tick), tail.to_vec(), sink);
            accepted += held;
            sink.stats.frames_queued += held as u64;
            dropped += tail.len() - held;
        }

        Self::settle(session, sink);

        let report = DeltaSendReport {
            frames_accepted: accepted,
            frames_dropped: dropped,
            fragments,
            superseded_queued: superseded,
        };
        if report.delivered() {
            sink.stats.deltas_sent += 1;
        }
        report
    }
}
