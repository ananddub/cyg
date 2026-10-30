//! # Voice Wire Packet & Opus Framing
//!
//! Compact binary wire framing for gaming voice packets:
//! [1 byte MAGIC 'V'][4 byte speaker_id][2 byte seq][4 byte timestamp][1 byte channel_mode][1 byte flags][2 byte len]\[payload\]
//! Total header overhead: exactly 15 bytes.

use bytes::{Buf, BufMut, Bytes, BytesMut};

pub const VOICE_MAGIC: u8 = 0x56; // ASCII 'V'
pub const HEADER_LEN: usize = 15;

/// Voice channel transmission modes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(u8)]
pub enum VoiceChannelMode {
    /// Spatial 3D proximity chat (attenuated by distance via KD-Tree).
    #[default]
    Proximity = 0,
    /// Broadcast to all listeners in room.
    Global = 1,
    /// Broadcast only to players sharing the same team ID.
    TeamOnly = 2,
    /// Targeted direct whisper to a specific entity.
    Whisper = 3,
}

impl VoiceChannelMode {
    pub fn from_u8(val: u8) -> Self {
        match val {
            1 => Self::Global,
            2 => Self::TeamOnly,
            3 => Self::Whisper,
            _ => Self::Proximity,
        }
    }
}

/// Bitfield flags in voice packet.
pub mod voice_flags {
    pub const STEREO: u8 = 1 << 0;
    pub const VOICE_ACTIVITY: u8 = 1 << 1;
    pub const PRIORITY_SPEAKER: u8 = 1 << 2;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VoicePacketError {
    BufferTooShort {
        expected: usize,
        actual: usize,
    },
    InvalidMagic(u8),
    PayloadLengthMismatch {
        header_len: usize,
        actual_len: usize,
    },
}

impl std::fmt::Display for VoicePacketError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::BufferTooShort { expected, actual } => {
                write!(
                    f,
                    "voice packet buffer too short: expected at least {expected}, got {actual}"
                )
            }
            Self::InvalidMagic(m) => write!(
                f,
                "invalid voice packet magic byte: 0x{m:02X} (expected 'V')"
            ),
            Self::PayloadLengthMismatch {
                header_len,
                actual_len,
            } => {
                write!(
                    f,
                    "voice packet payload length mismatch: header specifies {header_len}, buffer has {actual_len}"
                )
            }
        }
    }
}

impl std::error::Error for VoicePacketError {}

/// An in-flight Opus audio packet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VoicePacket {
    /// Speaker entity ID (player id).
    pub speaker_id: u32,
    /// Monotonic packet sequence counter for packet loss and jitter calculation.
    pub seq: u16,
    /// RTP-style timestamp (e.g. sample counter at 48 kHz).
    pub timestamp: u32,
    /// Transmission mode (Proximity, Team, Global, Whisper).
    pub channel_mode: VoiceChannelMode,
    /// Bitwise flags (stereo, VAD, priority).
    pub flags: u8,
    /// Opus-encoded audio frame (zero-copy Bytes refcounted slice).
    pub payload: Bytes,
}

impl VoicePacket {
    pub fn new(
        speaker_id: u32,
        seq: u16,
        timestamp: u32,
        channel_mode: VoiceChannelMode,
        flags: u8,
        payload: Bytes,
    ) -> Self {
        Self {
            speaker_id,
            seq,
            timestamp,
            channel_mode,
            flags,
            payload,
        }
    }

    /// Total serialized size including the 15-byte header.
    pub fn wire_size(&self) -> usize {
        HEADER_LEN + self.payload.len()
    }

    /// Encode into an allocated or pooled `BytesMut` buffer.
    pub fn encode_into(&self, buf: &mut BytesMut) {
        buf.reserve(self.wire_size());
        buf.put_u8(VOICE_MAGIC);
        buf.put_u32(self.speaker_id);
        buf.put_u16(self.seq);
        buf.put_u32(self.timestamp);
        buf.put_u8(self.channel_mode as u8);
        buf.put_u8(self.flags);
        buf.put_u16(self.payload.len() as u16);
        buf.put_slice(&self.payload);
    }

    /// Encode into a fresh zero-copy `Bytes` object.
    pub fn encode(&self) -> Bytes {
        let mut buf = BytesMut::with_capacity(self.wire_size());
        self.encode_into(&mut buf);
        buf.freeze()
    }

    /// Decode from raw wire bytes with zero-copy payload slicing.
    pub fn decode(bytes: &Bytes) -> Result<Self, VoicePacketError> {
        if bytes.len() < HEADER_LEN {
            return Err(VoicePacketError::BufferTooShort {
                expected: HEADER_LEN,
                actual: bytes.len(),
            });
        }

        let magic = bytes[0];
        if magic != VOICE_MAGIC {
            return Err(VoicePacketError::InvalidMagic(magic));
        }

        let mut r = &bytes[1..HEADER_LEN];
        let speaker_id = r.get_u32();
        let seq = r.get_u16();
        let timestamp = r.get_u32();
        let channel_mode = VoiceChannelMode::from_u8(r.get_u8());
        let flags = r.get_u8();
        let payload_len = r.get_u16() as usize;

        if bytes.len() < HEADER_LEN + payload_len {
            return Err(VoicePacketError::PayloadLengthMismatch {
                header_len: payload_len,
                actual_len: bytes.len() - HEADER_LEN,
            });
        }

        // Subslice zero-copy Bytes: bump refcount, no data copy.
        let payload = bytes.slice(HEADER_LEN..HEADER_LEN + payload_len);

        Ok(Self {
            speaker_id,
            seq,
            timestamp,
            channel_mode,
            flags,
            payload,
        })
    }

    /// Check if packet indicates Voice Activity Detection (speaker is talking).
    pub fn is_voice_active(&self) -> bool {
        (self.flags & voice_flags::VOICE_ACTIVITY) != 0
    }

    /// Check if packet is stereo.
    pub fn is_stereo(&self) -> bool {
        (self.flags & voice_flags::STEREO) != 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_voice_packet_roundtrip() {
        let raw_opus = Bytes::from_static(b"\x78\x00\x11\x22\x33\x44\x55\x66\x77\x88");
        let packet = VoicePacket::new(
            42,
            105,
            48000,
            VoiceChannelMode::Proximity,
            voice_flags::VOICE_ACTIVITY | voice_flags::STEREO,
            raw_opus.clone(),
        );

        let encoded = packet.encode();
        assert_eq!(encoded.len(), HEADER_LEN + raw_opus.len());

        let decoded = VoicePacket::decode(&encoded).expect("must decode cleanly");
        assert_eq!(decoded.speaker_id, 42);
        assert_eq!(decoded.seq, 105);
        assert_eq!(decoded.timestamp, 48000);
        assert_eq!(decoded.channel_mode, VoiceChannelMode::Proximity);
        assert!(decoded.is_voice_active());
        assert!(decoded.is_stereo());
        assert_eq!(decoded.payload, raw_opus);
    }

    #[test]
    fn test_invalid_magic_rejected() {
        let mut buf = BytesMut::from(&[0x00u8; 20][..]);
        buf[0] = 0x99; // Not 'V'
        let err = VoicePacket::decode(&buf.freeze()).unwrap_err();
        assert!(matches!(err, VoicePacketError::InvalidMagic(0x99)));
    }
}
