use super::StateSnapshot;
use crate::error::SidecarError;

pub trait AgonesController {
    fn mark_ready(&self) -> Result<(), SidecarError>;
    fn send_health_ping(&self) -> Result<(), SidecarError>;
    fn mark_allocated(&self) -> Result<(), SidecarError>;
    fn shutdown(&self) -> Result<(), SidecarError>;
    fn state(&self) -> Result<StateSnapshot, SidecarError>;
}
