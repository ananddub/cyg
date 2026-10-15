//! The socket's reply queue and its generation-checked teardown.

use tokio::sync::mpsc::Receiver;

use super::frames::SignalOut;
use super::hub::SignalHub;

/// A socket's reply queue; teardown on drop, so a cancelled task leaves no dead queue.
pub struct SeatReplies {
    pub(super) rx: Receiver<SignalOut>,
    pub(super) seat: String,
    pub(super) attach_id: u64,
    pub(super) hub: SignalHub,
}

impl SeatReplies {
    /// The next reply for this seat, or `None` once every sender is gone.
    pub async fn next(&mut self) -> Option<SignalOut> {
        self.rx.recv().await
    }
}

impl Drop for SeatReplies {
    fn drop(&mut self) {
        self.hub.detach(&self.seat, self.attach_id);
    }
}
