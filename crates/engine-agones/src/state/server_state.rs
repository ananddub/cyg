use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ServerState {
    Scheduled,
    Starting,
    Ready,
    NotReady,
    Reserved,
    Allocated,
    Shutdown,
    Unknown,
}

impl ServerState {
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
