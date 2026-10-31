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
static SESSIONS: OnceLock<SessionCache> = OnceLock::new();

struct Keys {
    signing: Vec<u8>,
    token_expiry_secs: i64,
}

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

pub fn token_expiry_secs() -> i64 {
    keys().token_expiry_secs
}

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

pub fn verify(token: &str) -> Result<ConsoleClaims, V2Error> {
    decode::<ConsoleClaims>(
        token,
        &DecodingKey::from_secret(&keys().signing),
        &Validation::default(),
    )
    .map(|data| data.claims)
    .map_err(|_| V2Error::unauthenticated("Console token invalid"))
}

pub fn revoke(id: Option<Uuid>, exp: i64, jti: &str) {
    sessions().remove(id.unwrap_or_else(Uuid::nil), exp, jti, 0, "");
}

pub fn is_live(id: Option<Uuid>, exp: i64, jti: &str) -> bool {
    sessions().is_valid_session(id.unwrap_or_else(Uuid::nil), exp, jti)
}

fn sessions() -> &'static SessionCache {
    SESSIONS.get_or_init(|| {
        let window = token_expiry_secs();
        SessionCache::new(window, window)
    })
}
