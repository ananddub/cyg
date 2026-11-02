use bytes::{Buf, BufMut, Bytes, BytesMut};

use super::{
    MAX_SIGNAL_PAYLOAD, SIGNAL_HEADER_LEN, SIGNAL_MAGIC, SIGNAL_VERSION, SignalError, SignalKind,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignalFrame {
    pub version: u8,
    pub kind: SignalKind,
    pub from: u32,
    pub to: u32,
    pub payload: Bytes,
}

impl SignalFrame {
    pub fn new(kind: SignalKind, from: u32, to: u32, payload: Bytes) -> Self {
        Self {
            version: SIGNAL_VERSION,
            kind,
            from,
            to,
            payload,
        }
    }

    #[inline]
    pub fn is_signal(buf: &[u8]) -> bool {
        buf.len() >= SIGNAL_HEADER_LEN && buf[0] == SIGNAL_MAGIC
    }

    #[inline]
    pub fn peek_kind(buf: &[u8]) -> Option<SignalKind> {
        if !Self::is_signal(buf) {
            return None;
        }
        SignalKind::from_u8(buf[2])
    }

    pub fn decode(buf: &Bytes) -> Result<Self, SignalError> {
        if buf.len() < SIGNAL_HEADER_LEN {
            return Err(SignalError::BufferTooShort {
                expected: SIGNAL_HEADER_LEN,
                actual: buf.len(),
            });
        }
        if buf[0] != SIGNAL_MAGIC {
            return Err(SignalError::InvalidMagic(buf[0]));
        }
        if buf[1] != SIGNAL_VERSION {
            return Err(SignalError::UnsupportedVersion(buf[1]));
        }
        let kind = SignalKind::from_u8(buf[2]).ok_or(SignalError::UnknownKind(buf[2]))?;
        let body = buf.len() - SIGNAL_HEADER_LEN;
        if body > MAX_SIGNAL_PAYLOAD {
            return Err(SignalError::PayloadTooLarge {
                limit: MAX_SIGNAL_PAYLOAD,
                actual: body,
            });
        }
        let mut cursor = &buf[3..SIGNAL_HEADER_LEN];
        Ok(Self {
            version: buf[1],
            kind,
            from: cursor.get_u32(),
            to: cursor.get_u32(),
            payload: buf.slice(SIGNAL_HEADER_LEN..),
        })
    }

    pub fn encode_into(&self, buf: &mut BytesMut) {
        buf.reserve(SIGNAL_HEADER_LEN + self.payload.len());
        buf.put_u8(SIGNAL_MAGIC);
        buf.put_u8(self.version);
        buf.put_u8(self.kind as u8);
        buf.put_u32(self.from);
        buf.put_u32(self.to);
        buf.put_slice(&self.payload);
    }

    pub fn encode(&self) -> Bytes {
        let mut buf = BytesMut::with_capacity(SIGNAL_HEADER_LEN + self.payload.len());
        self.encode_into(&mut buf);
        buf.freeze()
    }

    #[inline]
    pub fn wire_size(&self) -> usize {
        SIGNAL_HEADER_LEN + self.payload.len()
    }
}

pub fn rewrite_sender(frame: &Bytes, authoritative_from: u32) -> Bytes {
    debug_assert!(frame.len() >= SIGNAL_HEADER_LEN);
    let mut buf = BytesMut::with_capacity(frame.len());
    buf.put_slice(&frame[..3]);
    buf.put_u32(authoritative_from);
    buf.put_slice(&frame[7..]);
    buf.freeze()
}
