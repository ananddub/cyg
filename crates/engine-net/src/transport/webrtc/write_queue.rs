use std::collections::VecDeque;

use str0m::channel::ChannelId;

use super::WebRtcServer;
use super::contract::{MAX_QUEUED_FRAMES, OutboundDelta, Lane};
use super::session::{Sink, WebRtcSession};
use super::types::WriteAttempt;

impl WebRtcServer {
    pub(super) fn drain_queue(session: &mut WebRtcSession, lane: Lane, sink: &mut Sink) {
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

    pub(super) fn write_or_queue(
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

        Self::enqueue(session, lane, seq, tick, vec![frame.to_vec()], sink);
        sink.stats.frames_queued += 1;
        true
    }

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

            Ok(true) => WriteAttempt::Accepted,
            Ok(false) => WriteAttempt::Saturated,
            Err(_) => WriteAttempt::Lost,
        }
    }
}
