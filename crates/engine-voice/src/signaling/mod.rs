pub const SIGNAL_MAGIC: u8 = 0x57;
pub const SIGNAL_VERSION: u8 = 1;
pub const SIGNAL_HEADER_LEN: usize = 11;

pub const MAX_SIGNAL_PAYLOAD: usize = 64 * 1024;

pub const SERVER_SENDER: u32 = 0;

pub const BROADCAST: u32 = u32::MAX;

mod error;
mod frame;
mod kind;
mod mesh;

#[cfg(test)]
mod tests;

pub use error::SignalError;
pub use frame::{SignalFrame, rewrite_sender};
pub use kind::SignalKind;
pub use mesh::{SignalTarget, VoiceMesh};
