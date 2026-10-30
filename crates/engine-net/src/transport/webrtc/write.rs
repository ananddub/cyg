//! Getting one frame onto one lane: write it, or hold it in that lane's bounded
//! queue when SCTP's send buffer is full.

use std::collections::VecDeque;

use str0m::channel::ChannelId;

use super::WebRtcServer;
use super::contract::{FRAME_MAX_WIRE_LEN, Lane, MAX_QUEUED_FRAMES, OutboundDelta};
use super::session::{Sink, WebRtcSession};
use super::types::WriteAttempt;

impl WebRtcServer {
    pub(super) fn send_frame(&mut self, lane: Lane, client_id: &str, data: &[u8]) -> bool {
        if data.len() > FRAME_MAX_WIRE_LEN {
            self.sink.stats.frames_refused_oversize += 1;
            tracing::error!(
                "[webrtc] ⚠️ refused {} B on {} — MTU budget is {} B, split it (send_delta)",
                data.len(),
                lane.label(),
                FRAME_MAX_WIRE_LEN
            );
            return false;
        }
        let Some(session) = self.sessions.get_mut(client_id) else {
            return false;
        };
        let sent = Self::write_or_queue(session, lane, data, None, None, &mut self.sink);
        Self::settle(session, &mut self.sink);
        sent
    }

    pub(super) fn broadcast_frame(&mut self, lane: Lane, data: &[u8]) -> usize {
        if data.len() > FRAME_MAX_WIRE_LEN {
            self.sink.stats.frames_refused_oversize += 1;
            return 0;
        }
        let mut count = 0;
        for session in self.sessions.values_mut() {
            if Self::write_or_queue(session, lane, data, None, None, &mut self.sink) {
                count += 1;
            }
            Self::settle(session, &mut self.sink);
        }
        count
    }

    /// Retry every lane queue that still has an open channel. Each pass pops a
    /// whole delta and stops at the first lane that is still saturated, so one
    /// slow client cannot add unbounded work to a tick.
    pub(super) fn flush_outbound(&mut self) {
        for session in self.sessions.values_mut() {
            for lane in [Lane::Unreliable, Lane::Reliable] {
                Self::drain_queue(session, lane, &mut self.sink);
            }
            Self::settle(session, &mut self.sink);
        }
    }

    fn drain_queue(session: &mut WebRtcSession, lane: Lane, sink: &mut Sink) {
        let Some(cid) = session.channel_id(lane) else {
            if !session.queue(lane).is_empty() {
                let n = session.queue(lane).clear();
                sink.stats.frames_dropped_no_channel += n as u64;
            }
            return;
        };
        while let Some(delta) = session.queue(lane).pop() {
            let total = delta.frames.len();
            let mut written = 0usize;
            let mut saturated_at: Option<usize> = None;
            let mut lost = false;
            for (idx, frame) in delta.frames.iter().enumerate() {
                match Self::try_write(session, cid, frame) {
                    WriteAttempt::Accepted => {
                        sink.stats.frames_sent += 1;
                        written += 1;
                    }
                    WriteAttempt::Saturated => {
                        saturated_at = Some(idx);
                        break;
                    }
                    WriteAttempt::Lost => {
                        lost = true;
                        break;
                    }
                }
            }
            if written == total {
                continue;
            }
            match saturated_at {
                Some(idx) => {
                    // Reinsert the undelivered tail at the head: the next drain must not skip past a frame still waiting.
                    let tail =
                        VecDeque::from(delta.frames.into_iter().skip(idx).collect::<Vec<_>>());
                    session.queue(lane).push_front(OutboundDelta {
                        seq: delta.seq,
                        tick: delta.tick,
                        frames: tail,
                    });
                }
                None => {
                    let dropped = total - written;
                    if lost {
                        sink.stats.write_errors += 1;
                    }
                    sink.stats.frames_dropped_no_channel += dropped as u64;
                }
            }
            break;
        }
    }

    /// Write one frame, or hold it if the lane is full. `true` means the frame
    /// reached SCTP or this session's queue — not that it reached the client.
    fn write_or_queue(
        session: &mut WebRtcSession,
        lane: Lane,
        frame: &[u8],
        seq: Option<u16>,
        tick: Option<u32>,
        sink: &mut Sink,
    ) -> bool {
        let Some(cid) = session.channel_id(lane) else {
            sink.stats.frames_dropped_no_channel += 1;
            return false;
        };
        // Lane order is part of what the reliable channel promises, so a frame arriving behind held frames joins the back of the queue.
        if session.queue(lane).is_empty() {
            match Self::try_write(session, cid, frame) {
                WriteAttempt::Accepted => {
                    sink.stats.frames_sent += 1;
                    return true;
                }
                WriteAttempt::Lost => {
                    sink.stats.write_errors += 1;
                    sink.stats.frames_dropped_no_channel += 1;
                    return false;
                }
                WriteAttempt::Saturated => {}
            }
        }
        // A single frame always fits a queue of this capacity, so it is either held here or it evicts something older: it is never silently lost.
        Self::enqueue(session, lane, seq, tick, vec![frame.to_vec()], sink);
        sink.stats.frames_queued += 1;
        true
    }

    /// Queue frames as one delta unit, counting what the cap forced out. Returns
    /// how many of *these* frames the queue is now holding.
    pub(super) fn enqueue(
        session: &mut WebRtcSession,
        lane: Lane,
        seq: Option<u16>,
        tick: Option<u32>,
        frames: Vec<Vec<u8>>,
        sink: &mut Sink,
    ) -> usize {
        let count = frames.len();
        let (dropped_deltas, dropped_frames) = session.queue(lane).push(
            OutboundDelta {
                seq,
                tick,
                frames: VecDeque::from(frames),
            },
            MAX_QUEUED_FRAMES,
        );
        sink.stats.deltas_dropped_saturated += dropped_deltas;
        sink.stats.frames_dropped_saturated += dropped_frames;
        if count > MAX_QUEUED_FRAMES { 0 } else { count }
    }

    pub(super) fn try_write(
        session: &mut WebRtcSession,
        cid: ChannelId,
        frame: &[u8],
    ) -> WriteAttempt {
        let Some(mut channel) = session.rtc.channel(cid) else {
            return WriteAttempt::Lost;
        };
        match channel.write(true, frame) {
            // `Ok(false)` is str0m's own buffer check refusing the message: counting it as success is how a delta vanishes while the host still believes it was sent.
            Ok(true) => WriteAttempt::Accepted,
            Ok(false) => WriteAttempt::Saturated,
            Err(_) => WriteAttempt::Lost,
        }
    }
}
