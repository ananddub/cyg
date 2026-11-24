use std::sync::Arc;

use tokio::sync::{mpsc, oneshot};

use super::inbox::{PlaneCommand, SessionInbound};
use super::registries::Registries;
use super::stats::DemuxStats;
use super::types::{RtcDownlink, RtcPlaneError, UplinkEvent};

#[derive(Clone)]
pub struct PlaneHandle {
    pub(crate) cmd_tx: mpsc::UnboundedSender<PlaneCommand>,
    pub(crate) registries: Registries,
    pub(crate) uplink: Arc<tokio::sync::Mutex<mpsc::Receiver<UplinkEvent>>>,
    pub(crate) stats: DemuxStats,
}

impl PlaneHandle {
    pub async fn create_session(
        &self,
        session_id: Arc<str>,
        player_id: Arc<str>,
    ) -> Result<String, RtcPlaneError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.cmd_tx
            .send(PlaneCommand::CreateSession {
                session_id,
                player_id,
                reply: reply_tx,
            })
            .map_err(|_| RtcPlaneError::PlaneGone)?;
        reply_rx.await.map_err(|_| RtcPlaneError::PlaneGone)?
    }

    pub async fn apply_answer(
        &self,
        session_id: Arc<str>,
        sdp: String,
    ) -> Result<(), RtcPlaneError> {
        self.forward(session_id, SessionInbound::Answer { sdp })
            .await
    }

    pub async fn add_candidate(
        &self,
        session_id: Arc<str>,
        candidate: String,
    ) -> Result<(), RtcPlaneError> {
        self.forward(session_id, SessionInbound::Candidate { sdp: candidate })
            .await
    }

    pub async fn close_session(&self, session_id: Arc<str>) -> Result<(), RtcPlaneError> {
        self.forward(session_id, SessionInbound::Close).await
    }

    async fn forward(
        &self,
        session_id: Arc<str>,
        msg: SessionInbound,
    ) -> Result<(), RtcPlaneError> {
        self.cmd_tx
            .send(PlaneCommand::Forward { session_id, msg })
            .map_err(|_| RtcPlaneError::PlaneGone)
    }

    pub fn send_delta(&self, downlink: &RtcDownlink) -> bool {
        self.route_to_session(&downlink.player_id, |tx| {
            tx.try_send(SessionInbound::Delta {
                seq: downlink.seq,
                tick: downlink.tick,
                body: bytes::Bytes::copy_from_slice(&downlink.body),
            })
            .is_ok()
        })
    }

    pub fn send_reliable(&self, player_id: &Arc<str>, body: &[u8]) -> bool {
        let body = bytes::Bytes::copy_from_slice(body);
        self.route_to_session(player_id, move |tx| {
            tx.try_send(SessionInbound::Reliable { body: body.clone() })
                .is_ok()
        })
    }

    fn route_to_session(
        &self,
        player_id: &Arc<str>,
        send: impl FnOnce(&mpsc::Sender<SessionInbound>) -> bool,
    ) -> bool {
        let session_id = match self.registries.player_to_session(player_id) {
            Some(sid) => sid,
            None => return false,
        };
        self.registries
            .route_inbox(&session_id, send)
            .unwrap_or(false)
    }

    pub fn session_for_player(&self, player_id: &str) -> Option<Arc<str>> {
        self.registries.player_to_session(player_id)
    }

    pub fn session_count(&self) -> usize {
        self.registries.session_count()
    }

    /// Packets the demux dropped because a session inbox was full — the
    /// bounded-512 backpressure path, now observable.
    pub fn dropped_inbound(&self) -> u64 {
        self.stats.dropped_inbound()
    }

    pub async fn recv_uplink(&self) -> Option<UplinkEvent> {
        self.uplink.lock().await.recv().await
    }
}
