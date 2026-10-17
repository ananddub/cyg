//! Authorization extraction for `/v2` and `/ws`, ported from the gateway's
//! security interceptor, which splits credentials in two: `Authenticate*` and
//! `SessionRefresh` prove the caller with the static server key, everything else
//! proves who the player is with a session token — and the refusal names which
//! kind of credential was missing.

use axum::extract::FromRequestParts;
use axum::http::HeaderMap;
use axum::http::request::Parts;
use base64::Engine;
use uuid::Uuid;

use crate::error::V2Error;
use crate::infra::console::{ConsoleClaims, ConsoleRole};
use crate::infra::jwt::Claims;

/// A verified session. Constructed by the extractor, never by hand.
#[derive(Debug, Clone)]
pub struct Authed(pub Claims);

impl Authed {
    /// The session's user id. [`crate::infra::jwt::verify_session`] refuses a token whose
    /// `uid` is not a UUID, so a constructed `Authed` always carries a real one.
    pub fn uid(&self) -> Uuid {
        self.0.uuid().expect("verified session user id")
    }
}

/// Pull the bearer token out of a header map, exactly as BaaS does: the prefix
/// is `"Bearer "` and case-sensitive, so `"bearer …"` is no credential at all.
pub fn bearer_token(headers: &HeaderMap) -> Option<&str> {
    let value = headers
        .get(axum::http::header::AUTHORIZATION)?
        .to_str()
        .ok()?;
    value.strip_prefix("Bearer ")
}

/// The count of `Authorization` headers. A repeated header is treated as a
/// malformed credential rather than a pick-one, so "absent" and "ambiguous" stay
/// distinct.
fn auth_headers(headers: &HeaderMap) -> usize {
    headers
        .get_all(axum::http::header::AUTHORIZATION)
        .iter()
        .count()
}

impl<S: Send + Sync> FromRequestParts<S> for Authed {
    type Rejection = V2Error;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        // The extractor is state-agnostic so auto-registered controllers can
        // use it under a `Router<()>`; the plane state comes from the installed
        // global.
        Ok(Authed(session_claims(
            &parts.headers,
            &crate::current_state(),
        )?))
    }
}

/// Prove the caller holds a live session token.
///
/// A function rather than only an extractor because the gateway runs its
/// interceptor *after* it has decoded the body: a malformed body answers 400 even
/// when it carries no token at all, and handlers that need that order parse the
/// body first and call this.
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

/// Prove the caller knows the server key. See [`session_claims`] for why this
/// is callable directly.
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
    if parse_basic_user(value).as_deref() != Some(state.control.session.server_key.as_str()) {
        return Err(V2Error::unauthenticated("Server key invalid"));
    }
    Ok(())
}

/// `Basic <base64("user:pass")>` → the user part. The password half is unused,
/// but the colon must be there: the parser rejects a payload without one.
fn parse_basic_user(value: &str) -> Option<String> {
    let encoded = value.strip_prefix("Basic ")?;
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .ok()?;
    let (user, _) = std::str::from_utf8(&decoded).ok()?.split_once(':')?;
    Some(user.to_owned())
}

/// A verified console session. Constructed by the extractor, never by hand.
#[derive(Debug, Clone)]
pub struct ConsoleAuthed(pub ConsoleClaims);

impl ConsoleAuthed {
    pub fn role(&self) -> ConsoleRole {
        self.0.role()
    }

    /// Refuse a mutating call from a read-only console user. The status is the one
    /// the failure kind maps to, so the refusal reads like every other.
    pub fn require_write(&self) -> Result<(), V2Error> {
        if self.role().can_write() {
            Ok(())
        } else {
            Err(V2Error::permission_denied(
                "This console user has read-only access.",
            ))
        }
    }

    /// Refuse everything but an admin. Managing operators, settings and deleting
    /// data are the operations a mistake cannot be undone from, so they are not
    /// shared with the write roles.
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
        // Same reason as [`Authed`]: state-agnostic so a controller can use it
        // under a `Router<()>`; the console realm is process-wide.
        Ok(ConsoleAuthed(console_claims(&parts.headers)?))
    }
}

/// Prove the caller holds a live console token. The raw token is its own
/// revocation handle, so it is carried into the liveness check rather than hashed.
pub fn console_claims(headers: &HeaderMap) -> Result<ConsoleClaims, V2Error> {
    if auth_headers(headers) != 1 {
        return Err(V2Error::unauthenticated("Console token required"));
    }
    let invalid = || V2Error::unauthenticated("Console token invalid");
    let token = bearer_token(headers).ok_or_else(invalid)?;
    let claims = crate::infra::console::verify(token)?;
    if !crate::infra::console::is_live(claims.uuid(), claims.exp, &claims.jti) {
        return Err(invalid());
    }
    Ok(claims)
}
