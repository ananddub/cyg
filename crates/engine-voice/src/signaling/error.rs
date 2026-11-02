#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SignalError {
    BufferTooShort { expected: usize, actual: usize },
    InvalidMagic(u8),
    UnsupportedVersion(u8),
    UnknownKind(u8),
    PayloadTooLarge { limit: usize, actual: usize },
}

impl std::fmt::Display for SignalError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::BufferTooShort { expected, actual } => {
                write!(
                    f,
                    "signal frame too short: expected at least {expected}, got {actual}"
                )
            }
            Self::InvalidMagic(m) => {
                write!(f, "invalid signal magic byte: 0x{m:02X} (expected 'W')")
            }
            Self::UnsupportedVersion(v) => write!(f, "unsupported signal protocol version {v}"),
            Self::UnknownKind(k) => write!(f, "unknown signal kind: {k}"),
            Self::PayloadTooLarge { limit, actual } => {
                write!(
                    f,
                    "signal body too large: {actual} bytes exceeds limit of {limit}"
                )
            }
        }
    }
}

impl std::error::Error for SignalError {}
