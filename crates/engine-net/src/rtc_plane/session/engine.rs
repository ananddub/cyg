use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use str0m::change::{SdpAnswer, SdpPendingOffer};
use str0m::channel::ChannelId;
use str0m::net::Protocol;
use str0m::{Candidate, Rtc};

use super::channels::{reliable_config, unreliable_config};
use super::types::{Outbound, PlaneChannel};
use super::{LABEL_RELIABLE, LABEL_UNRELIABLE};
use crate::rtc_plane::RtcPlaneError;

const WIRE_MTU_TARGET: usize = 1100;

pub struct RtcSession {
    pub(super) rtc: Rtc,

    pub(super) pending: Option<SdpPendingOffer>,

    pub(super) local_addr: SocketAddr,

    pub(super) session_id: Arc<str>,

    pub(super) ch_unreliable: Option<ChannelId>,
    pub(super) ch_reliable: Option<ChannelId>,
    pub(super) channel_labels: HashMap<ChannelId, String>,

    pub(super) remote_addr: Option<SocketAddr>,
    pub(super) connected: bool,
    pub(super) next_timeout: Instant,
}

impl RtcSession {
    pub fn new_server(
        session_id: Arc<str>,
        local_addr: SocketAddr,
        candidates: &[SocketAddr],
    ) -> Result<(Self, String), RtcPlaneError> {
        let now = Instant::now();
        let mut rtc = Rtc::builder()
            .set_mtu(WIRE_MTU_TARGET..=WIRE_MTU_TARGET + 100)
            .build(now);

        for addr in candidates {
            let cand = Candidate::host(*addr, Protocol::Udp)
                .map_err(|e| RtcPlaneError::Signaling(format!("bad candidate {addr}: {e:?}")))?;
            rtc.add_local_candidate(cand);
        }

        let mut api = rtc.sdp_api();
        let ch_unreliable = api.add_channel_with_config(unreliable_config());
        let ch_reliable = api.add_channel_with_config(reliable_config());
        let (offer, pending) = api
            .apply()
            .ok_or_else(|| RtcPlaneError::Signaling("no negotiation produced".into()))?;
        let offer_sdp = offer.to_sdp_string();

        let mut session = RtcSession {
            rtc,
            pending: Some(pending),
            local_addr,
            session_id,
            ch_unreliable: Some(ch_unreliable),
            ch_reliable: Some(ch_reliable),
            channel_labels: HashMap::new(),
            remote_addr: None,
            connected: false,
            next_timeout: now + Duration::from_secs(3600),
        };

        let _ = session.drain();
        Ok((session, offer_sdp))
    }

    pub fn accept_answer(&mut self, sdp: &str) -> Result<Vec<Outbound>, RtcPlaneError> {
        let answer = SdpAnswer::from_sdp_string(sdp)
            .map_err(|e| RtcPlaneError::Signaling(format!("invalid answer: {e:?}")))?;
        let pending = self.pending.take().ok_or(RtcPlaneError::Signaling(
            "no pending offer to answer".into(),
        ))?;
        self.rtc
            .sdp_api()
            .accept_answer(pending, answer)
            .map_err(|e| RtcPlaneError::Signaling(format!("accept_answer failed: {e:?}")))?;
        Ok(self.drain())
    }

    pub fn add_remote_candidate(&mut self, sdp: &str) -> Result<Vec<Outbound>, RtcPlaneError> {
        let trimmed = sdp.trim().trim_start_matches("a=").trim();
        let candidate = Candidate::from_sdp_string(trimmed)
            .or_else(|_| {
                let stripped = trimmed.trim_start_matches("candidate:").trim();
                Candidate::from_sdp_string(stripped)
            })
            .map_err(|e| RtcPlaneError::Signaling(format!("bad candidate: {e:?}")))?;
        self.rtc.add_remote_candidate(candidate);
        Ok(self.drain())
    }

    pub fn is_connected(&self) -> bool {
        self.connected
    }

    pub fn session_id(&self) -> &Arc<str> {
        &self.session_id
    }

    pub fn remote_addr(&self) -> Option<SocketAddr> {
        self.remote_addr
    }

    pub fn channel_ids(&self) -> (Option<ChannelId>, Option<ChannelId>) {
        (self.ch_unreliable, self.ch_reliable)
    }

    pub fn channel_role(&self, id: ChannelId) -> Option<PlaneChannel> {
        if self.ch_unreliable == Some(id) {
            return Some(PlaneChannel::Unreliable);
        }
        if self.ch_reliable == Some(id) {
            return Some(PlaneChannel::Reliable);
        }
        self.channel_labels
            .get(&id)
            .and_then(|label| match label.as_str() {
                LABEL_UNRELIABLE => Some(PlaneChannel::Unreliable),
                LABEL_RELIABLE => Some(PlaneChannel::Reliable),
                _ => None,
            })
    }
}
