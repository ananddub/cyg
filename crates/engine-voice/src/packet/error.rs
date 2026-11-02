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
