pub const PER_SEAT_QUEUE: usize = 64;

pub const INBOUND_QUEUE: usize = 4096;

pub(super) fn mpsc_channel<T>(
    cap: usize,
) -> (tokio::sync::mpsc::Sender<T>, tokio::sync::mpsc::Receiver<T>) {
    tokio::sync::mpsc::channel(cap)
}
