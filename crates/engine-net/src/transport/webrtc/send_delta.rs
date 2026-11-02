use super::WebRtcServer;
use super::contract::Lane;
use super::session::{Sink, WebRtcSession};
use super::types::{DeltaSendReport, WriteAttempt};

impl WebRtcServer {
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
