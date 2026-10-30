//! Downlink writes: the game's delta and reliable payloads into str0m channels.

use engine_webrtc_protocol::channel::{MAX_DELTA_CHUNK, delta_fragment_count, write_delta_frame};

use super::engine::RtcSession;
use super::types::Outbound;
use super::{LABEL_RELIABLE, LABEL_UNRELIABLE};
use crate::rtc_plane::RtcPlaneError;

impl RtcSession {
    /// Fragment a delta body with the codec and write every frame to the
    /// unreliable channel. Never hands str0m a blob larger than `MAX_FRAME`.
    ///
    /// Returns the datagrams the engine produced as a result (may be empty until
    /// the channel is open). An oversized body (beyond `MAX_DELTA_BODY`) is
    /// rejected rather than silently truncated.
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
        let mut frame = Vec::new();
        for idx in 0..total {
            let start = idx as usize * MAX_DELTA_CHUNK;
            let end = (start + MAX_DELTA_CHUNK).min(body.len());
            write_delta_frame(&mut frame, seq, tick, idx, total, &body[start..end]);
            if let Some(mut ch) = self.rtc.channel(cid) {
                // A full unreliable buffer makes str0m drop the newest frame, not grow.
                let _ = ch.write(true, &frame);
            }
            // Mutate -> drain, per the single-mutation invariant.
            out.extend(self.drain());
        }
        Ok(out)
    }

    /// Write a whole payload to the reliable channel (caller owns framing; the
    /// event layer does not fragment on this channel — SCTP orders + resends).
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
