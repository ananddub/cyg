use axum::extract::FromRequestParts;
use axum::http::HeaderMap;
use axum::http::request::Parts;
use base64::Engine;
use tracing::Instrument;
use uuid::Uuid;

use crate::error::V2Error;
use crate::infra::console::{ConsoleClaims, ConsoleRole};
use crate::infra::jwt::Claims;

#[derive(Debug, Clone)]
pub struct Authed(pub Claims);

impl Authed {
    pub fn uid(&self) -> Uuid {
        self.0.uuid().expect("verified session user id")
    }
}

pub fn bearer_token(headers: &HeaderMap) -> Option<&str> {
    let value = headers
        .get(axum::http::header::AUTHORIZATION)?
        .to_str()
        .ok()?;
    value.strip_prefix("Bearer ")
}

fn auth_headers(headers: &HeaderMap) -> usize {
    headers
        .get_all(axum::http::header::AUTHORIZATION)
        .iter()
        .count()
}

impl<S: Send + Sync> FromRequestParts<S> for Authed {
    type Rejection = V2Error;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        Ok(Authed(session_claims(
            &parts.headers,
            &crate::current_state(),
        )?))
    }
}

pub fn session_claims(headers: &HeaderMap, state: &crate::ApiState) -> Result<Claims, V2Error> {
    match auth_headers(headers) {
        0 => return Err(V2Error::unauthenticated("Auth token required")),
        1 => {}
        _ => return Err(V2Error::unauthenticated("Auth token invalid")),
    }
    let invalid = || V2Error::unauthenticated("Auth token invalid");
    let token = bearer_token(headers).ok_or_else(invalid)?;
    let claims = crate::infra::jwt::verify_session(token)?;
    let uid = claims.uuid()?;
    if !state
        .sessions
        .is_valid_session(uid, claims.exp, &claims.tid)
    {
        return Err(invalid());
    }
    Ok(claims)
}

pub fn server_key(headers: &HeaderMap, state: &crate::ApiState) -> Result<(), V2Error> {
    if auth_headers(headers) != 1 {
        return Err(V2Error::unauthenticated("Server key required"));
    }
    let Some(value) = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
    else {
        return Err(V2Error::unauthenticated("Server key invalid"));
    };
    let Some(presented) = parse_basic_user(value) else {
        return Err(V2Error::unauthenticated("Server key invalid"));
    };
    // The retired key stays valid for one rotation window so a backend that has
    // not redeployed yet is not cut off mid-rotation; `!=` would leak the key one
    // byte at a time to anyone willing to measure attempts.
    if !engine_config::Rotation::global()
        .accepts_server_key(&state.control.session.server_key, &presented)
    {
        return Err(V2Error::unauthenticated("Server key invalid"));
    }
    Ok(())
}

/// Privileged server-to-server routes accept either the server key (game
/// backend) or a console session; a player session is never enough.
pub fn server_key_or_console(headers: &HeaderMap, state: &crate::ApiState) -> Result<(), V2Error> {
    if server_key(headers, state).is_ok() {
        return Ok(());
    }
    console_claims(headers).map(|_| ())
}

fn parse_basic_user(value: &str) -> Option<String> {
    let encoded = value.strip_prefix("Basic ")?;
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .ok()?;
    let (user, _) = std::str::from_utf8(&decoded).ok()?.split_once(':')?;
    Some(user.to_owned())
}

#[derive(Debug, Clone)]
pub struct ConsoleAuthed(pub ConsoleClaims);

impl ConsoleAuthed {
    pub fn role(&self) -> ConsoleRole {
        self.0.role()
    }

    pub fn require_write(&self) -> Result<(), V2Error> {
        if self.role().can_write() {
            Ok(())
        } else {
            Err(V2Error::permission_denied(
                "This console user has read-only access.",
            ))
        }
    }

    pub fn require_admin(&self) -> Result<(), V2Error> {
        if self.role().is_admin() {
            Ok(())
        } else {
            Err(V2Error::permission_denied(
                "This console action requires the admin role.",
            ))
        }
    }
}

impl<S: Send + Sync> FromRequestParts<S> for ConsoleAuthed {
    type Rejection = V2Error;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        Ok(ConsoleAuthed(console_claims(&parts.headers)?))
    }
}

pub fn console_claims(headers: &HeaderMap) -> Result<ConsoleClaims, V2Error> {
    if auth_headers(headers) != 1 {
        return Err(V2Error::unauthenticated("Console token required"));
    }
    let token =
        bearer_token(headers).ok_or_else(|| V2Error::unauthenticated("Console token invalid"))?;
    console_token_claims(token)
}

/// Browsers cannot set an `Authorization` header on a WebSocket handshake, so
/// the dashboard socket carries the same console token as a query parameter.
pub fn console_token_claims(token: &str) -> Result<ConsoleClaims, V2Error> {
    let invalid = || V2Error::unauthenticated("Console token invalid");
    let claims = crate::infra::console::verify(token)?;
    if !crate::infra::console::is_live(claims.uuid(), claims.exp, &claims.jti) {
        return Err(invalid());
    }
    Ok(claims)
}

pub const REQUEST_ID_HEADER: &str = "x-request-id";
const MAX_INCOMING_REQUEST_ID_LEN: usize = 128;

/// Accepts a client-supplied id only when it is safe to echo back verbatim
/// (printable ASCII, bounded length); anything else gets a fresh uuid v4.
fn sanitize_request_id(raw: Option<&str>) -> Option<String> {
    let raw = raw?;
    if raw.len() > MAX_INCOMING_REQUEST_ID_LEN
        || !raw.bytes().all(|b| b.is_ascii_graphic() || b == b' ')
    {
        return None;
    }
    Some(raw.to_owned())
}

pub async fn request_id(
    request: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    let id = sanitize_request_id(
        request
            .headers()
            .get(REQUEST_ID_HEADER)
            .and_then(|v| v.to_str().ok()),
    )
    .unwrap_or_else(|| Uuid::new_v4().to_string());
    let span = tracing::info_span!("http", request_id = %id, method = %request.method());
    let mut response = next.run(request).instrument(span).await;
    if let Ok(value) = axum::http::HeaderValue::from_str(&id) {
        response.headers_mut().insert(REQUEST_ID_HEADER, value);
    }
    response
}
