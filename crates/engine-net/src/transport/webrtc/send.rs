
use super::WebRtcServer;
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
