use tokio::sync::mpsc::Receiver;

use super::frames::SignalOut;
use super::hub::SignalHub;

pub struct SeatReplies {
    pub(super) rx: Receiver<SignalOut>,
    pub(super) seat: String,
    pub(super) attach_id: u64,
    pub(super) hub: SignalHub,
}

impl SeatReplies {
    pub async fn next(&mut self) -> Option<SignalOut> {
        self.rx.recv().await
    }
}

impl Drop for SeatReplies {
    fn drop(&mut self) {
        self.hub.detach(&self.seat, self.attach_id);
    }
}
