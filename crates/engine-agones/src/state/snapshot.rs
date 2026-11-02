use serde::{Deserialize, Serialize};

use super::ServerState;
use crate::error::SidecarError;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct StateSnapshot {
    pub state: ServerState,
    #[serde(rename = "stateRaw")]
    pub state_raw: String,
    #[serde(default)]
    pub metadata: serde_json::Value,
}

impl StateSnapshot {
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

    pub fn display(&self) -> String {
        if self.state == ServerState::Unknown && !self.state_raw.is_empty() {
            format!("Unknown({})", self.state_raw)
        } else {
            self.state.to_string()
        }
    }
}

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
