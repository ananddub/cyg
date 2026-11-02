use serde::de::DeserializeOwned;
use serde_json::Value;

pub struct Msg {
    cid: Option<String>,
    event: String,
    payload: Value,
}

impl Msg {
    pub fn new(cid: Option<String>, event: impl Into<String>, payload: Value) -> Self {
        Self {
            cid,
            event: event.into(),
            payload,
        }
    }

    pub fn cid(&self) -> Option<&str> {
        self.cid.as_deref()
    }

    pub fn event(&self) -> &str {
        &self.event
    }

    pub fn raw(&self) -> &Value {
        &self.payload
    }

    pub fn payload<T: DeserializeOwned>(&self) -> Result<T, WsError> {
        serde_json::from_value(self.payload.clone())
            .map_err(|error| WsError::new(format!("invalid payload for {}: {error}", self.event)))
    }
}

#[derive(Debug, Clone)]
pub struct WsError {
    pub message: String,
}

impl WsError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl std::fmt::Display for WsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for WsError {}
