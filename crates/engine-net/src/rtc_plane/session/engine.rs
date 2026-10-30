//! Session identity and negotiation: the `str0m::Rtc` engine, the SDP offer/answer
//! that creates it, the two DataChannel contracts, and the read-only accessors.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use str0m::change::{SdpAnswer, SdpPendingOffer};
use str0m::channel::{ChannelConfig, ChannelId, Reliability};
use str0m::net::Protocol;
use str0m::{Candidate, Rtc};

use super::types::{Outbound, PlaneChannel};
use super::{LABEL_RELIABLE, LABEL_UNRELIABLE};
use crate::rtc_plane::RtcPlaneError;

/// Wire MTU budget str0m sizes SCTP/DTLS records to. The codec already keeps each
/// DataChannel message under `MAX_FRAME` (1200); this target leaves headroom for
/// SCTP + DTLS framing so no *datagram* exceeds the 1200-byte path budget.
const WIRE_MTU_TARGET: usize = 1100;

/// A single browser peer's WebRTC session, driven by its own Tokio task.
pub struct RtcSession {
    pub(super) rtc: Rtc,
    /// The offer we produced, awaiting the remote answer to complete negotiation.
    pub(super) pending: Option<SdpPendingOffer>,
    /// Destination address we stamp on inbound [`str0m::net::Receive`]s. str0m only
    /// accepts a datagram whose destination matches one of our local candidates, so
    /// for a socket bound to `0.0.0.0` the demux passes the concrete announced
    /// address.
    pub(super) local_addr: SocketAddr,

    pub(super) session_id: Arc<str>,

    pub(super) ch_unreliable: Option<ChannelId>,
    pub(super) ch_reliable: Option<ChannelId>,
    pub(super) channel_labels: HashMap<ChannelId, String>,

    pub(super) remote_addr: Option<SocketAddr>,
    pub(super) connected: bool,
    pub(super) next_timeout: Instant,
}

/// Unreliable channel config: unordered, 0-retransmits.
///
/// The browser `RTCDataChannelInit` cannot request both `maxRetransmits: 0` and a
/// `maxPacketLifeTime` at once (they are mutually exclusive per the WebRTC spec),
/// so we honor the codec's documented `{ ordered: false, maxRetransmits: 0 }`
/// contract — pure fire-and-forget. A stale tick is superseded by the next tick
/// within ~20 ms, which is tighter than any packet lifetime we would pick.
fn unreliable_config() -> ChannelConfig {
    ChannelConfig {
        label: LABEL_UNRELIABLE.to_string(),
        ordered: false,
        reliability: Reliability::MaxRetransmits { retransmits: 0 },
        ..Default::default()
    }
}

/// Reliable channel config: ordered with unlimited retransmits (`Reliable`).
fn reliable_config() -> ChannelConfig {
    ChannelConfig {
        label: LABEL_RELIABLE.to_string(),
        ordered: true,
        reliability: Reliability::Reliable,
        ..Default::default()
    }
}

impl RtcSession {
    /// Create a server session that *offers* both game DataChannels and returns
    /// the SDP offer string to hand to the remote browser via signaling.
    ///
    /// `candidates` are the addresses the server advertises as host ICE
    /// candidates; `local_addr` is the concrete destination address inbound
    /// datagrams will carry (used to build [`str0m::net::Receive`]).
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
        // Priming the drain is what schedules the first DTLS/ICE timeout.
        let _ = session.drain();
        Ok((session, offer_sdp))
    }

    /// Apply the remote SDP answer to the offer we produced.
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

    /// Feed a remote ICE candidate (SDP `candidate:` line) to the engine.
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

    /// Which channel id is which, once opened.
    pub fn channel_ids(&self) -> (Option<ChannelId>, Option<ChannelId>) {
        (self.ch_unreliable, self.ch_reliable)
    }

    /// Resolve a channel id back to its game role (for uplink routing).
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
