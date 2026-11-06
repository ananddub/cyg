
use super::WebRtcServer;
use engine_webrtc_protocol::{MAX_DELTA_BODY, delta_fragment_count};

use super::contract::{DeltaTooLarge, Lane, split_delta};
use super::types::DeltaSendReport;

impl WebRtcServer {

    pub fn send_delta(
        &mut self,
        client_id: &str,
        seq: u16,
        tick: u32,
        body: &[u8],
    ) -> Result<DeltaSendReport, DeltaTooLarge> {
        // Oversize is refused BEFORE anything else — the accounting must hold
        // even for sessions that will never reach a lane.
        let fragment_count = match delta_fragment_count(body.len()) {
            Some(count) => count as usize,
            None => {
                self.sink.stats.deltas_refused_oversized += 1;
                return Err(DeltaTooLarge::BodyTooLarge {
                    body_len: body.len(),
                    max: MAX_DELTA_BODY,
                });
            }
        };
        // Reject non-WebRTC sessions BEFORE splitting: WS/UDP clients' deltas
        // ride their own lanes, and splitting was allocating ~3 Vecs per send
        // only to drop the frames immediately (the engine's #1 churn site).
        let Some(session) = self.sessions.get_mut(client_id) else {
            self.sink.stats.frames_dropped_no_channel += fragment_count as u64;
            return Ok(DeltaSendReport::skipped(fragment_count));
        };
        let mut frames = Vec::new();
        let fragments = match split_delta(seq, tick, body, &mut frames) {
            Ok(fragments) => fragments,
            Err(err) => {
                self.sink.stats.deltas_refused_oversized += 1;
                return Err(err);
            }
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

    pub fn broadcast_delta(&mut self, seq: u16, tick: u32, body: &[u8]) -> usize {
        if self.sessions.is_empty() {
            if delta_fragment_count(body.len()).is_none() {
                self.sink.stats.deltas_refused_oversized += 1;
            }
            return 0; // nobody to broadcast to — don't split the body
        }
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

    pub fn send_unreliable(&mut self, client_id: &str, data: &[u8]) -> bool {
        self.send_frame(Lane::Unreliable, client_id, data)
    }

    pub fn send_reliable(&mut self, client_id: &str, data: &[u8]) -> bool {
        self.send_frame(Lane::Reliable, client_id, data)
    }

    pub fn broadcast_unreliable(&mut self, data: &[u8]) -> usize {
        self.broadcast_frame(Lane::Unreliable, data)
    }

    pub fn broadcast_reliable(&mut self, data: &[u8]) -> usize {
        self.broadcast_frame(Lane::Reliable, data)
    }

}
