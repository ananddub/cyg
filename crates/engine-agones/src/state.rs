//! The lifecycle state a sidecar reports, and the trait every controller
//! implements.
//!
//! Variant names are the Agones spellings verbatim, so serde writes exactly what
//! `GET /state` writes and a dashboard can compare strings without a table.

use serde::{Deserialize, Serialize};

use crate::error::SidecarError;

/// Lifecycle phase of this GameServer, as observed.
///
/// `Unknown` is a real outcome, not a placeholder: the sidecar can report a state
/// this client was not built against (Agones adds phases between releases), and
/// guessing would be worse than saying so.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ServerState {
    /// Created, not yet scheduled to a node.
    Scheduled,
    /// Scheduled, container starting.
    Starting,
    /// In the pool, acceptable for allocation. Requires `/ready` then health.
    Ready,
    /// Failed its health check — three missed intervals do this.
    NotReady,
    /// Reserved for a specific allocation request, not yet handed out.
    Reserved,
    /// A client owns this pod.
    Allocated,
    /// Told to retire; the agent deletes the pod.
    Shutdown,
    /// Anything this client does not recognise.
    Unknown,
}

impl ServerState {
    /// Parse a sidecar state string, case- and separator-insensitively.
    /// Unrecognised input becomes [`ServerState::Unknown`] rather than an error:
    /// `/state` is telemetry, and telemetry must not fail because a name changed.
    pub fn parse(raw: &str) -> ServerState {
        let key: String = raw
            .trim()
            .chars()
            .filter(|c| !c.is_whitespace() && *c != '-' && *c != '_')
            .collect();
        match key.to_ascii_lowercase().as_str() {
            "scheduled" => ServerState::Scheduled,
            "starting" => ServerState::Starting,
            "ready" => ServerState::Ready,
            "notready" => ServerState::NotReady,
            "reserved" => ServerState::Reserved,
            "allocated" => ServerState::Allocated,
            "shutdown" => ServerState::Shutdown,
            _ => ServerState::Unknown,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            ServerState::Scheduled => "Scheduled",
            ServerState::Starting => "Starting",
            ServerState::Ready => "Ready",
            ServerState::NotReady => "NotReady",
            ServerState::Reserved => "Reserved",
            ServerState::Allocated => "Allocated",
            ServerState::Shutdown => "Shutdown",
            ServerState::Unknown => "Unknown",
        }
    }

    /// A pod is in a state where a health ping keeps it alive.
    pub fn health_matters(self) -> bool {
        matches!(
            self,
            ServerState::Ready | ServerState::Allocated | ServerState::Reserved
        )
    }
}

impl std::fmt::Display for ServerState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Everything `GET /state` tells us: the phase plus the metadata the agent
/// attaches (name, namespace, and after allocation, the client `ip`/`port`).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct StateSnapshot {
    /// The parsed phase.
    pub state: ServerState,
    /// The raw string the sidecar sent, kept so an unknown name is still visible.
    #[serde(rename = "stateRaw")]
    pub state_raw: String,
    /// Passed through verbatim; `{}` when the sidecar sent none.
    #[serde(default)]
    pub metadata: serde_json::Value,
}

impl StateSnapshot {
    /// Parse the sidecar's reply.
    ///
    /// A function rather than a `Deserialize` impl, because the wire shape is not
    /// this type: the sidecar sends `state` **once**, and we keep both its raw
    /// spelling and our reading of it — and a name from a future Agones must become
    /// [`ServerState::Unknown`] instead of a deserialisation error, which would
    /// look like a broken sidecar.
    pub fn from_json(text: &str) -> Result<StateSnapshot, SidecarError> {
        #[derive(Deserialize)]
        struct Wire {
            #[serde(default)]
            state: String,
            #[serde(default)]
            metadata: serde_json::Value,
        }

        let wire: Wire =
            serde_json::from_str(text.trim()).map_err(|e| SidecarError::Malformed {
                reason: format!(
                    "`state` payload was not usable JSON ({e}): {}",
                    trim_for_log(text)
                ),
            })?;
        let state_raw = wire.state.trim().to_string();
        if state_raw.is_empty() {
            return Err(SidecarError::Malformed {
                reason: "`state` payload carried no lifecycle state".into(),
            });
        }
        let metadata = if wire.metadata.is_null() {
            serde_json::json!({})
        } else {
            wire.metadata
        };
        Ok(StateSnapshot {
            state: ServerState::parse(&state_raw),
            state_raw,
            metadata,
        })
    }

    /// The phase if the sidecar's own string parsed, else the raw text. This is
    /// what a dashboard should print.
    pub fn display(&self) -> String {
        if self.state == ServerState::Unknown && !self.state_raw.is_empty() {
            format!("Unknown({})", self.state_raw)
        } else {
            self.state.to_string()
        }
    }
}

/// A short, single-line excerpt of a reply for an error message.
fn trim_for_log(text: &str) -> String {
    let flat: String = text
        .chars()
        .map(|c| if c == '\n' || c == '\r' { ' ' } else { c })
        .collect();
    let mut end = flat.len().min(120);
    while end > 0 && !flat.is_char_boundary(end) {
        end -= 1;
    }
    flat[..end].to_string()
}

/// The lifecycle surface every controller offers.
///
/// `&self`, not `&mut self`: these calls are idempotent transitions against an
/// external authority, so there is no local state to guard, and an `Arc`'d client
/// can be shared between the health thread and whatever handles allocation.
pub trait AgonesController {
    /// `POST /ready` — enter the allocation pool.
    fn mark_ready(&self) -> Result<(), SidecarError>;
    /// `POST /health` — one heartbeat.
    fn send_health_ping(&self) -> Result<(), SidecarError>;
    /// `POST /allocated` — record the allocation handoff.
    fn mark_allocated(&self) -> Result<(), SidecarError>;
    /// `POST /shutdown` — retire this pod. Idempotent by contract: a second call
    /// must not turn into a second transition.
    fn shutdown(&self) -> Result<(), SidecarError>;
    /// `GET /state` — what the sidecar thinks right now.
    fn state(&self) -> Result<StateSnapshot, SidecarError>;
}
