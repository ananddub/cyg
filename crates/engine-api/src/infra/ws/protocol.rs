use auto_ws::{Identity, Msg, WsError};
use axum::extract::Query;
use axum::http::request::Parts;
use axum::response::{IntoResponse, Response};

use crate::error::V2Error;
use crate::infra::jwt;

use super::envelope::{Envelope, EnvelopeMessage, ErrorMsg};

#[derive(Debug, serde::Deserialize)]
struct WsQuery {
    #[serde(default)]
    token: Option<String>,
}

pub struct WsProtocol;

impl auto_ws::Protocol for WsProtocol {
    fn authenticate(parts: &Parts) -> Result<Identity, Response> {
        let state = crate::current_state();
        let Query(WsQuery { token }) =
            Query::<WsQuery>::try_from_uri(&parts.uri).map_err(IntoResponse::into_response)?;
        let token = token
            .filter(|token| !token.is_empty())
            .or_else(|| bearer(parts))
            .ok_or_else(|| {
                V2Error::unauthenticated("no session token: pass ?token=<jwt>").into_response()
            })?;
        let claims = jwt::verify_session(&token).map_err(IntoResponse::into_response)?;
        let uid = claims.uuid().map_err(IntoResponse::into_response)?;
        Ok(Identity::new(uid, claims.un).with(state))
    }

    fn decode(frame: &str) -> Result<Msg, WsError> {
        let envelope: Envelope = serde_json::from_str(frame)
            .map_err(|error| WsError::new(format!("cannot parse envelope: {error}")))?;
        let payload = serde_json::to_value(&envelope.message)
            .map_err(|error| WsError::new(format!("cannot parse envelope: {error}")))?;
        let event = payload
            .get("type")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .to_string();
        Ok(Msg::new(envelope.cid, event, payload))
    }

    fn encode_error(cid: Option<&str>, error: &WsError) -> String {
        serde_json::to_string(&Envelope {
            cid: cid.map(str::to_string),
            message: EnvelopeMessage::Error(ErrorMsg {
                message: error.message.clone(),
            }),
        })
        .unwrap_or_else(|_| "{\"type\":\"error\",\"message\":\"cannot encode error\"}".to_string())
    }
}

fn bearer(parts: &Parts) -> Option<String> {
    parts
        .headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| {
            value
                .strip_prefix("Bearer ")
                .or_else(|| value.strip_prefix("bearer "))
        })
        .map(|token| token.to_string())
}

impl From<V2Error> for WsError {
    fn from(error: V2Error) -> Self {
        WsError::new(error.message)
    }
}

impl From<WsError> for V2Error {
    fn from(error: WsError) -> Self {
        V2Error::internal(error.message)
    }
}
