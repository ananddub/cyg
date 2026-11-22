use std::net::SocketAddr;

use axum::extract::{ConnectInfo, FromRequestParts};
use axum::http::HeaderMap;
use axum::http::request::Parts;
use axum::response::IntoResponse;
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
        // `player_rate_limit` verifies every request that carries a session
        // token while picking its bucket; reuse that instead of paying for a
        // second JWT check per handler.
        if let Some(authed) = parts.extensions.get::<Authed>() {
            return Ok(authed.clone());
        }
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
    console_claims(headers, state, crate::current_remote()).map(|_| ())
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
        Ok(ConsoleAuthed(console_claims(
            &parts.headers,
            &crate::current_state(),
            remote_addr(parts),
        )?))
    }
}

/// The address the request arrived from, when the server tracks it.
pub fn remote_addr(parts: &Parts) -> Option<SocketAddr> {
    parts
        .extensions
        .get::<ConnectInfo<SocketAddr>>()
        .map(|connect| connect.0)
}

pub fn console_claims(
    headers: &HeaderMap,
    state: &crate::ApiState,
    remote: Option<SocketAddr>,
) -> Result<ConsoleClaims, V2Error> {
    // `cyg run -d`: the operator asked for a local session. Making them log in
    // to their own machine is a loop with no secret in it — but the grant is
    // only safe when the request really comes from this machine: the flag was
    // resolved once at boot and the source address must be loopback, so a
    // misconfigured bind cannot hand admin to a remote peer.
    if dev_admin(state, remote) {
        return Ok(ConsoleClaims {
            id: String::new(),
            username: "dev".into(),
            email: String::new(),
            role: ConsoleRole::Admin.as_u8(),
            jti: String::new(),
            exp: i64::MAX,
        });
    }
    if auth_headers(headers) != 1 {
        return Err(V2Error::unauthenticated("Console token required"));
    }
    let token =
        bearer_token(headers).ok_or_else(|| V2Error::unauthenticated("Console token invalid"))?;
    console_token_claims(token)
}

/// Whether this request may ride the dev-session admin grant: the process was
/// started with `cyg run -d` (resolved once at boot into the API state) and the
/// request arrived over the loopback interface.
fn dev_admin(state: &crate::ApiState, remote: Option<SocketAddr>) -> bool {
    state.dev_mode && remote.is_some_and(|addr| addr.ip().is_loopback())
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

/// The account a request speaks for. A token that verifies as neither a player
/// session nor an unexempted operator is the handler's problem, not the
/// limiter's: it answers with the refusal its own route already gives.
fn bucketed(headers: &HeaderMap, state: &crate::ApiState, remote: Option<SocketAddr>) -> Option<Uuid> {
    if auth_headers(headers) != 1 {
        return None;
    }
    if let Ok(claims) = session_claims(headers, state) {
        return claims.uuid().ok();
    }
    if state.control.quotas.exempt_console {
        return None;
    }
    console_claims(headers, state, remote).ok()?.uuid()
}

/// Paths whose handler runs Argon2 (or an equivalent password check) per call.
/// Each attempt is expensive for the server and silent for the attacker, so
/// they draw from a tighter per-address bucket than the rest of the plane.
fn password_path(path: &str) -> bool {
    path.starts_with("/v2/account/authenticate") || path.starts_with("/v2/console/authenticate")
}

/// Matchmake routes charge the same per-address bucket inside
/// `MatchmakeService::reserve` (direct callers and the seat tests rely on it).
/// Charging here too would take two tokens per HTTP join, so the outer bucket
/// leaves these to the service.
fn matchmake_path(path: &str) -> bool {
    path.starts_with("/matchmake/") || path.starts_with("/api/matchmake/")
}

/// Paths that take work from a player. Health, readyz, status, dashboard and
/// statics stay reachable while the host is still booting or draining, so an
/// operator can see *why* the server is not taking joins.
fn work_path(path: &str) -> bool {
    path.starts_with("/matchmake/")
        || path.starts_with("/api/matchmake/")
        || path.starts_with("/queue/")
        || path.starts_with("/api/queue/")
        || path == "/ws"
        || path == "/ws/signal"
}

/// Don't accept work until start-up is complete, and stop accepting it once a
/// drain begins. This is the application-level version of a readiness gate:
/// bind early, serve probes, but fail joins fast with a retryable 503 instead
/// of queueing them behind a booting/draining host.
pub async fn lifecycle_gate(
    request: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    if !work_path(request.uri().path()) {
        return next.run(request).await;
    }
    if let Some(manager) = crate::app::state::lifecycle::manager()
        && !manager.is_serving()
    {
        let phase = manager.phase();
        return (
            axum::http::StatusCode::SERVICE_UNAVAILABLE,
            [(axum::http::header::RETRY_AFTER, "1")],
            axum::body::Body::from(format!("server is {}; retry shortly", phase.as_str())),
        )
            .into_response();
    }
    next.run(request).await
}

/// The outer per-address bucket, applied to every route before any other
/// check: unauthenticated traffic has no account to bucket, so the source
/// address is the only thing the limiter can charge. The general plane bucket
/// is the operator's configured `rate_limit_rps`/`rate_limit_burst`; the
/// password routes above draw from a much tighter one.
pub async fn ip_rate_limit(
    request: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    let state = crate::current_state();
    let Some(remote) = request
        .extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .map(|connect| connect.0)
    else {
        return next.run(request).await;
    };
    let ip = remote.ip();
    let path = request.uri().path();
    if matchmake_path(path) {
        return next.run(request).await;
    }
    if password_path(path) {
        if !state.limiter.allow_auth(ip) {
            let secs = state.limiter.retry_after_auth(ip).max(1);
            return V2Error::resource_exhausted(
                "Too many sign-in attempts from this address; slow down.",
            )
            .with_retry_after(secs)
            .into_response();
        }
        return next.run(request).await;
    }
    if !state.limiter.allow(ip) {
        let secs = state.limiter.retry_after(ip).max(1);
        return V2Error::resource_exhausted("This address is asking too quickly.")
            .with_retry_after(secs)
            .into_response();
    }
    next.run(request).await
}

/// The per-account request bucket, applied to the whole plane instead of to the
/// expensive handlers one by one. Off with the quotas.
pub async fn player_rate_limit(
    mut request: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    let state = crate::current_state();
    if !state.control.quotas.enabled {
        return next.run(request).await;
    }
    // One verification per request: the claims the bucket decision needed are
    // stashed here so the `Authed` extractor does not verify the token again.
    let claims = session_claims(request.headers(), &state).ok();
    if let Some(claims) = claims.clone() {
        request.extensions_mut().insert(Authed(claims));
    }
    if let Some(refusal) = rate_refusal(&state, request.headers(), claims.as_ref()) {
        return refusal.into_response();
    }
    next.run(request).await
}

/// `None` means the request goes through — either it has tokens left or it is
/// not the limiter's to judge. `verified` carries the session claims already
/// checked by `player_rate_limit`, if any.
fn rate_refusal(
    state: &crate::ApiState,
    headers: &HeaderMap,
    verified: Option<&crate::infra::jwt::Claims>,
) -> Option<V2Error> {
    let uid = match verified.and_then(|claims| claims.uuid().ok()) {
        Some(uid) => Some(uid),
        None => bucketed(headers, state, crate::current_remote()),
    };
    let uid = uid?;
    if state.limiter.allow_player(uid) {
        return None;
    }
    let secs = state.limiter.retry_after_player(uid).max(1);
    Some(V2Error::resource_exhausted("This account is asking too quickly.").with_retry_after(secs))
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
    let route = request
        .extensions()
        .get::<axum::extract::MatchedPath>()
        .map(|path| path.as_str().to_string());
    let span = tracing::info_span!(
        "http",
        request_id = %id,
        method = %request.method(),
        http.route = tracing::field::Empty,
        http.status_code = tracing::field::Empty,
    );
    if let Some(route) = route {
        span.record("http.route", route.as_str());
    }
    let mut response = next.run(request).instrument(span.clone()).await;
    span.record("http.status_code", response.status().as_u16());
    if let Ok(value) = axum::http::HeaderValue::from_str(&id) {
        response.headers_mut().insert(REQUEST_ID_HEADER, value);
    }
    response
}
