//! The caller-facing half of the plane: signaling relay and the non-blocking
//! downlink router.

use std::sync::Arc;

use tokio::sync::{mpsc, oneshot};

use super::inbox::{PlaneCommand, SessionInbound};
use super::registries::Registries;
use super::types::{RtcDownlink, RtcPlaneError, UplinkEvent};

/// Handle to the running plane, cloneable and `Send + Sync` for use from the
/// signaling thread and the game loop thread.
#[derive(Clone)]
pub struct PlaneHandle {
    pub(crate) cmd_tx: mpsc::UnboundedSender<PlaneCommand>,
    pub(crate) registries: Registries,
    pub(crate) uplink: Arc<tokio::sync::Mutex<mpsc::UnboundedReceiver<UplinkEvent>>>,
}

impl PlaneHandle {
    /// Create a session for `session_id` bound to `player_id`; returns the SDP
    /// offer to hand to the browser.
    pub async fn create_session(
        &self,
        session_id: Arc<str>,
        player_id: Arc<str>,
    ) -> Result<String, RtcPlaneError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.cmd_tx
            .send(PlaneCommand::CreateSession { session_id, player_id, reply: reply_tx })
            .map_err(|_| RtcPlaneError::PlaneGone)?;
        reply_rx.await.map_err(|_| RtcPlaneError::PlaneGone)?
    }

    /// Deliver the browser's SDP answer for a session we offered.
    pub async fn apply_answer(&self, session_id: Arc<str>, sdp: String) -> Result<(), RtcPlaneError> {
        self.forward(session_id, SessionInbound::Answer { sdp }).await
    }

    /// Deliver a trickle ICE candidate from the browser.
    pub async fn add_candidate(&self, session_id: Arc<str>, candidate: String) -> Result<(), RtcPlaneError> {
        self.forward(session_id, SessionInbound::Candidate { sdp: candidate }).await
    }

    /// Close and dispose a session.
    pub async fn close_session(&self, session_id: Arc<str>) -> Result<(), RtcPlaneError> {
        self.forward(session_id, SessionInbound::Close).await
    }

    async fn forward(&self, session_id: Arc<str>, msg: SessionInbound) -> Result<(), RtcPlaneError> {
        self.cmd_tx
            .send(PlaneCommand::Forward { session_id, msg })
            .map_err(|_| RtcPlaneError::PlaneGone)
    }

    /// Push a delta to a player. Non-blocking: if the session's bounded inbox is
    /// full the delta is dropped (a stale tick is superseded within ~20 ms anyway),
    /// so the game-loop producer is never stalled by a slow client. Returns whether
    /// the delta was accepted into the session inbox.
    pub fn send_delta(&self, downlink: &RtcDownlink) -> bool {
        self.route_to_session(&downlink.player_id, |tx| {
            tx.try_send(SessionInbound::Delta {
                seq: downlink.seq,
                tick: downlink.tick,
                body: Arc::from(downlink.body.as_slice()),
            })
            .is_ok()
        })
    }

    /// Push a payload to a player's reliable channel. Non-blocking like
    /// [`Self::send_delta`].
    pub fn send_reliable(&self, player_id: &Arc<str>, body: &[u8]) -> bool {
        let body: Arc<[u8]> = Arc::from(body);
        self.route_to_session(player_id, move |tx| {
            tx.try_send(SessionInbound::Reliable { body: body.clone() }).is_ok()
        })
    }

    /// Shared downlink routing: resolve player → session → inbox sender and apply
    /// a non-blocking `try_send`. Never blocks the producer; `false` if the player
    /// is unknown, the session is gone, or the inbox is full.
    fn route_to_session(
        &self,
        player_id: &Arc<str>,
        send: impl FnOnce(&mpsc::Sender<SessionInbound>) -> bool,
    ) -> bool {
        let session_id = match self.registries.player_to_session(player_id) {
            Some(sid) => sid,
            None => return false,
        };
        self.registries.route_inbox(&session_id, send).unwrap_or(false)
    }

    /// The "which session belongs to player id" lookup.
    pub fn session_for_player(&self, player_id: &str) -> Option<Arc<str>> {
        self.registries.player_to_session(player_id)
    }

    /// Number of live sessions with a registered inbox.
    pub fn session_count(&self) -> usize {
        self.registries.session_count()
    }

    /// Await the next uplink event. The bridge drains this onto its crossbeam bus.
    pub async fn recv_uplink(&self) -> Option<UplinkEvent> {
        self.uplink.lock().await.recv().await
    }
}
