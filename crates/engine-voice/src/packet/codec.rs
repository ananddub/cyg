use bytes::{Buf, BufMut, Bytes, BytesMut};

use super::{HEADER_LEN, VOICE_MAGIC, VoiceChannelMode, VoicePacketError, voice_flags};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VoicePacket {
    pub speaker_id: u32,
    pub seq: u16,
    pub timestamp: u32,
    pub channel_mode: VoiceChannelMode,
    pub flags: u8,
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

    pub fn wire_size(&self) -> usize {
        HEADER_LEN + self.payload.len()
    }

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

    pub fn encode(&self) -> Bytes {
        let mut buf = BytesMut::with_capacity(self.wire_size());
        self.encode_into(&mut buf);
        buf.freeze()
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, VoicePacketError> {
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

        let payload = Bytes::copy_from_slice(&bytes[HEADER_LEN..HEADER_LEN + payload_len]);

        Ok(Self {
            speaker_id,
            seq,
            timestamp,
            channel_mode,
            flags,
            payload,
        })
    }

    pub fn is_voice_active(&self) -> bool {
        (self.flags & voice_flags::VOICE_ACTIVITY) != 0
    }

    pub fn is_stereo(&self) -> bool {
        (self.flags & voice_flags::STEREO) != 0
    }
}
