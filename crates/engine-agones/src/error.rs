use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SidecarErrorKind {
    Unreachable,

    Timeout,

    Rejected,

    Malformed,
}

impl SidecarErrorKind {
    pub fn as_str(self) -> &'static str {
        match self {
            SidecarErrorKind::Unreachable => "unreachable",
            SidecarErrorKind::Timeout => "timeout",
            SidecarErrorKind::Rejected => "rejected",
            SidecarErrorKind::Malformed => "malformed",
        }
    }

    pub fn retryable(self) -> bool {
        !matches!(self, SidecarErrorKind::Rejected)
    }
}

impl fmt::Display for SidecarErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SidecarError {
    Unreachable {
        addr: String,
        reason: String,
    },

    Timeout {
        addr: String,
        after_ms: u64,
    },

    Rejected {
        status: u16,
        reason: String,
        body: String,
    },

    Malformed {
        reason: String,
    },
}

impl SidecarError {
    pub fn kind(&self) -> SidecarErrorKind {
        match self {
            SidecarError::Unreachable { .. } => SidecarErrorKind::Unreachable,
            SidecarError::Timeout { .. } => SidecarErrorKind::Timeout,
            SidecarError::Rejected { .. } => SidecarErrorKind::Rejected,
            SidecarError::Malformed { .. } => SidecarErrorKind::Malformed,
        }
    }

    pub fn message(&self) -> String {
        match self {
            SidecarError::Unreachable { addr, reason } => {
                format!("cannot reach sidecar at {addr}: {reason}")
            }
            SidecarError::Timeout { addr, after_ms } => {
                format!("sidecar at {addr} sent no complete reply within {after_ms} ms")
            }
            SidecarError::Rejected {
                status,
                reason,
                body,
            } => {
                if body.is_empty() {
                    format!("sidecar rejected the transition: HTTP {status} {reason}")
                } else {
                    format!("sidecar rejected the transition: HTTP {status} {reason} — {body}")
                }
            }
            SidecarError::Malformed { reason } => format!("sidecar reply was unusable: {reason}"),
        }
    }
}

impl fmt::Display for SidecarError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message())
    }
}

impl std::error::Error for SidecarError {}
