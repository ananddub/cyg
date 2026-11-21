use engine_webrtc_protocol::channel::{MAX_DELTA_CHUNK, delta_fragment_count, write_delta_frame};

use super::engine::RtcSession;
use super::types::Outbound;
use super::{LABEL_RELIABLE, LABEL_UNRELIABLE};
use crate::rtc_plane::RtcPlaneError;

impl RtcSession {
    pub fn send_delta(
        &mut self,
        seq: u16,
        tick: u32,
        body: &[u8],
    ) -> Result<Vec<Outbound>, RtcPlaneError> {
        let cid = self
            .ch_unreliable
            .ok_or(RtcPlaneError::ChannelNotOpen(LABEL_UNRELIABLE.into()))?;
        let total =
            delta_fragment_count(body.len()).ok_or(RtcPlaneError::DeltaTooLarge(body.len()))?;

        let mut out = Vec::new();
        for idx in 0..total {
            let start = idx as usize * MAX_DELTA_CHUNK;
            let end = (start + MAX_DELTA_CHUNK).min(body.len());
            // Reused scratch: one MTU frame per fragment, cleared not allocated.
            self.frame_scratch.clear();
            write_delta_frame(
                &mut self.frame_scratch,
                seq,
                tick,
                idx,
                total,
                &body[start..end],
            );
            if let Some(mut ch) = self.rtc.channel(cid) {
                let _ = ch.write(true, &self.frame_scratch);
            }

            out.extend(self.drain());
        }
        Ok(out)
    }

    pub fn send_reliable(&mut self, payload: &[u8]) -> Result<Vec<Outbound>, RtcPlaneError> {
        let cid = self
            .ch_reliable
            .ok_or(RtcPlaneError::ChannelNotOpen(LABEL_RELIABLE.into()))?;
        let mut out = Vec::new();
        if let Some(mut ch) = self.rtc.channel(cid) {
            ch.write(true, payload)
                .map_err(|e| RtcPlaneError::Transport(format!("reliable write: {e:?}")))?;
        }
        out.extend(self.drain());
        Ok(out)
    }
}
