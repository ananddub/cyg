pub const VOICE_MAGIC: u8 = 0x56;
pub const HEADER_LEN: usize = 15;

mod codec;
mod error;
mod mode;

#[cfg(test)]
mod tests;

pub use codec::VoicePacket;
pub use error::VoicePacketError;
pub use mode::{VoiceChannelMode, voice_flags};
