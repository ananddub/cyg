use std::fmt;

use crate::plane::reliable::consts::MAX_INFLIGHT_EVENTS;
use crate::plane::reliable::consts::MAX_RELIABLE_BYTES;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QueueError {
    WindowFull { inflight: usize },
    TooBig { bytes: usize },
    NotNewer { seq: u16, last: u16 },
}

impl fmt::Display for QueueError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            QueueError::WindowFull { inflight } => write!(
                f,
                "reliable window is full ({inflight}/{MAX_INFLIGHT_EVENTS} events await an ack)"
            ),
            QueueError::TooBig { bytes } => write!(
                f,
                "a reliable payload exceeds {MAX_RELIABLE_BYTES} bytes, got {bytes}"
            ),
            QueueError::NotNewer { seq, last } => {
                write!(f, "event seq {seq} is not newer than the queued {last}")
            }
        }
    }
}

impl std::error::Error for QueueError {}
