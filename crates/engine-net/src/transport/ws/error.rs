//! Typed matchmake rejections.

/// A typed matchmake rejection, mapped onto the Colyseus HTTP error codes
/// (shared-types ErrorCode) and surfaced to the SDK as a `MatchMakeError`.
#[derive(Debug, Clone)]
pub struct MatchMakeError {
    pub code: u16,
    pub message: String,
}

impl MatchMakeError {
    pub fn new(code: u16, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
    pub fn no_handler(message: impl Into<String>) -> Self {
        Self::new(520, message)
    }
    pub fn invalid_criteria(message: impl Into<String>) -> Self {
        Self::new(521, message)
    }
    pub fn invalid_room_id(message: impl Into<String>) -> Self {
        Self::new(522, message)
    }
    pub fn unhandled(message: impl Into<String>) -> Self {
        Self::new(523, message)
    }
    pub fn expired(message: impl Into<String>) -> Self {
        Self::new(524, message)
    }
    pub fn auth_failed(message: impl Into<String>) -> Self {
        Self::new(525, message)
    }
    pub fn application_error(message: impl Into<String>) -> Self {
        Self::new(526, message)
    }
}

impl From<&str> for MatchMakeError {
    fn from(message: &str) -> Self {
        Self::unhandled(message)
    }
}

impl From<String> for MatchMakeError {
    fn from(message: String) -> Self {
        Self::unhandled(message)
    }
}
