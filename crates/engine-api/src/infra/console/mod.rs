//! Console tokens: the admin console's own HS256 realm.
//!
//! A console token is signed with `[control.console].signing_key` and is never
//! accepted as a player session (nor the reverse): the two realms use different
//! keys, so holding one grants nothing on the other. Beyond `exp`, the role
//! travels inside the token, so an authorization check needs no database round-trip.

mod claims;
mod role;

use std::sync::OnceLock;

use chrono::Utc;
use engine_config::ConsoleConfig;
use jsonwebtoken::{DecodingKey, EncodingKey, Header, Validation, decode, encode};
use uuid::Uuid;

use crate::error::V2Error;
use crate::infra::session_cache::SessionCache;

pub use claims::ConsoleClaims;
pub use role::ConsoleRole;

static CONFIG: OnceLock<Keys> = OnceLock::new();
/// Revoked console tokens. Process-wide, as the console itself is: a logout in one
/// task must fail the same token in another, and there is exactly one console.
static SESSIONS: OnceLock<SessionCache> = OnceLock::new();

struct Keys {
    signing: Vec<u8>,
    token_expiry_secs: i64,
}

/// Called from [`crate::ApiState::new`] with `[control.console]`. One console per
/// process, so the first call wins and later calls are ignored.
pub fn configure(console: &ConsoleConfig) {
    let _ = CONFIG.set(Keys {
        signing: console.signing_key.as_bytes().to_vec(),
        token_expiry_secs: console.token_expiry_secs,
    });
}

fn keys() -> &'static Keys {
    CONFIG.get_or_init(|| {
        let default = ConsoleConfig::default();
        Keys {
            signing: default.signing_key.as_bytes().to_vec(),
            token_expiry_secs: default.token_expiry_secs,
        }
    })
}

/// The token lifetime the session cache is built around; the same window signs
/// `exp` and decides when a logout entry may be swept.
pub fn token_expiry_secs() -> i64 {
    keys().token_expiry_secs
}

/// Mint a console token for a verified console user. `id` is empty for the config
/// bootstrap superadmin, which has no `console_user` row.
pub fn mint(id: &str, username: &str, email: &str, role: ConsoleRole) -> String {
    let k = keys();
    let claims = ConsoleClaims {
        id: id.to_owned(),
        username: username.to_owned(),
        email: email.to_owned(),
        role: role.as_u8(),
        jti: Uuid::new_v4().simple().to_string(),
        exp: Utc::now().timestamp() + k.token_expiry_secs,
    };
    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(&k.signing),
    )
    .expect("console claims are encodable")
}

/// Verify a console token. A player session token fails here rather than needing a
/// marker: it is signed with the session key, which is a different secret.
pub fn verify(token: &str) -> Result<ConsoleClaims, V2Error> {
    decode::<ConsoleClaims>(
        token,
        &DecodingKey::from_secret(&keys().signing),
        &Validation::default(),
    )
    .map(|data| data.claims)
    .map_err(|_| V2Error::unauthenticated("Console token invalid"))
}

/// Blacklist one console token by its `jti`, so two logins in the same second are
/// revoked independently.
pub fn revoke(id: Option<Uuid>, exp: i64, jti: &str) {
    sessions().remove(id.unwrap_or_else(Uuid::nil), exp, jti, 0, "");
}

/// Whether a verified console token is still live (not logged out).
pub fn is_live(id: Option<Uuid>, exp: i64, jti: &str) -> bool {
    sessions().is_valid_session(id.unwrap_or_else(Uuid::nil), exp, jti)
}

fn sessions() -> &'static SessionCache {
    SESSIONS.get_or_init(|| {
        let window = token_expiry_secs();
        SessionCache::new(window, window)
    })
}
