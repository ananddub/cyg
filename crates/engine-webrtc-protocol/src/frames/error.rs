#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameError {
    TooShort {
        need: usize,
        got: usize,
    },
    UnknownOpcode(u8),
    UnknownEventKind(u8),

    LengthMismatch {
        want: usize,
        got: usize,
    },
    TooManyEntities {
        max: usize,
        got: usize,
    },
    EmptyState,
}

impl std::fmt::Display for FrameError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TooShort { need, got } => write!(f, "frame needs {need} bytes, got {got}"),
            Self::UnknownOpcode(v) => write!(f, "unassigned opcode 0x{v:02X}"),
            Self::UnknownEventKind(v) => write!(f, "unassigned event kind {v}"),
            Self::LengthMismatch { want, got } => {
                write!(f, "frame should be {want} bytes, got {got}")
            }
            Self::TooManyEntities { max, got } => {
                write!(f, "{got} entities in one state frame, max {max}")
            }
            Self::EmptyState => write!(f, "state frame with no entities"),
        }
    }
}

impl std::error::Error for FrameError {}
