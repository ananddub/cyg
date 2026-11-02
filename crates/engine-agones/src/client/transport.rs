use super::io::request;
use super::{Reply, SidecarConfig, truncate};
use crate::error::SidecarError;
use crate::state::{AgonesController, StateSnapshot};

#[derive(Debug, Clone)]
pub struct AgonesClient {
    config: SidecarConfig,
}

impl AgonesClient {
    pub fn new(config: SidecarConfig) -> AgonesClient {
        AgonesClient { config }
    }

    pub fn config(&self) -> &SidecarConfig {
        &self.config
    }

    pub fn call(&self, method: &str, path: &str) -> Result<Reply, SidecarError> {
        request(&self.config, method, path)
    }

    pub fn post(&self, path: &str) -> Result<Reply, SidecarError> {
        let reply = self.call("POST", path)?;
        if reply.is_success() {
            return Ok(reply);
        }
        Err(SidecarError::Rejected {
            status: reply.status,
            reason: reply.reason.clone(),
            body: truncate(&reply.body_lossy(), 200),
        })
    }

    pub fn get_state(&self) -> Result<StateSnapshot, SidecarError> {
        let reply = self.call("GET", "/state")?;
        if !reply.is_success() {
            return Err(SidecarError::Rejected {
                status: reply.status,
                reason: reply.reason.clone(),
                body: truncate(&reply.body_lossy(), 200),
            });
        }
        StateSnapshot::from_json(&reply.body_lossy())
    }
}

impl AgonesController for AgonesClient {
    fn mark_ready(&self) -> Result<(), SidecarError> {
        self.post("/ready").map(|_| ())
    }

    fn send_health_ping(&self) -> Result<(), SidecarError> {
        self.post("/health").map(|_| ())
    }

    fn mark_allocated(&self) -> Result<(), SidecarError> {
        self.post("/allocated").map(|_| ())
    }

    fn shutdown(&self) -> Result<(), SidecarError> {
        self.post("/shutdown").map(|_| ())
    }

    fn state(&self) -> Result<StateSnapshot, SidecarError> {
        self.get_state()
    }
}
