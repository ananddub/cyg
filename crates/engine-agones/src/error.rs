//! The three honest answers a sidecar call can get.
//!
//! A lifecycle client that collapses "no sidecar exists" and "the sidecar
//! refused this transition" into one error is useless to an operator: the first
//! means *fix the deployment*, the second means *the pod is already past this
//! state and that is normal*. These variants keep them apart, and none of them
//! can carry a panic: a malformed, truncated or non-UTF-8 reply is
//! [`SidecarError::Malformed`], never an unwrap.

use std::fmt;

/// What kind of failure this is, as a stable lowercase string for JSON.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SidecarErrorKind {
    /// The socket could not be opened, written or read at all: no sidecar on
    /// that port, wrong port, or the pod network is not up yet. Retryable.
    Unreachable,
    /// The sidecar accepted the connection but produced no complete HTTP reply
    /// before our deadline (a throttled kubelet does this). Retryable.
    Timeout,
    /// The sidecar answered with a 4xx/5xx: the transition itself was refused
    /// because the GameServer is not in a state that allows it. Usually *not*
    /// retryable by us — the agent owns that decision.
    Rejected,
    /// The reply was not parseable HTTP/JSON: truncated, bad status line, or
    /// bytes we cannot decode. Retryable, but worth a loud log.
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

    /// Whether keeping the loop alive is the right response. Everything except a
    /// rejected transition is: no sidecar, a late reply and an unreadable reply are
    /// all conditions that can pass, while a 400 means the GameServer itself is not
    /// in a state that allows this transition and re-sending it changes nothing.
    pub fn retryable(self) -> bool {
        !matches!(self, SidecarErrorKind::Rejected)
    }
}

impl fmt::Display for SidecarErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// One sidecar failure, with enough detail to log without a debugger.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SidecarError {
    /// Connect / write / resolve failed. `reason` is the OS message.
    Unreachable { addr: String, reason: String },
    /// No complete reply within the configured budget.
    Timeout { addr: String, after_ms: u64 },
    /// A 4xx/5xx status. `body` is the sidecar's own explanation, lossily
    /// decoded and truncated.
    Rejected { status: u16, reason: String, body: String },
    /// The reply could not be parsed. `reason` says where.
    Malformed { reason: String },
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

    /// A short human sentence, safe for a log line and for `/status`.
    pub fn message(&self) -> String {
        match self {
            SidecarError::Unreachable { addr, reason } => {
                format!("cannot reach sidecar at {addr}: {reason}")
            }
            SidecarError::Timeout { addr, after_ms } => {
                format!("sidecar at {addr} sent no complete reply within {after_ms} ms")
            }
            SidecarError::Rejected { status, reason, body } => {
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
