//! Queue sizes and the channel factory.

/// Replies buffered per socket before that client's signaling is wedged.
/// Signaling is low-rate: 64 is several multiples of a full ICE candidate sweep.
pub const PER_SEAT_QUEUE: usize = 64;

/// Shared capacity for everything heading to the data plane.
pub const INBOUND_QUEUE: usize = 4096;

pub(super) fn mpsc_channel<T>(
    cap: usize,
) -> (tokio::sync::mpsc::Sender<T>, tokio::sync::mpsc::Receiver<T>) {
    tokio::sync::mpsc::channel(cap)
}
