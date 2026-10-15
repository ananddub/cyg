//! Minting: the two signing keys, the two expiry windows, and the pair sharing one `tid`.

use std::collections::HashMap;
use std::sync::OnceLock;

use chrono::Utc;
use engine_config::SessionConfig;
use jsonwebtoken::{EncodingKey, Header, encode};
use uuid::Uuid;

use super::claims::Claims;

static CONFIG: OnceLock<Keys> = OnceLock::new();

pub(super) struct Keys {
    signing: Vec<u8>,
    refresh_signing: Vec<u8>,
    token_expiry_secs: i64,
    refresh_expiry_secs: i64,
}

/// Called from [`crate::ApiState::new`] with `[control.session]`. One control
/// plane per process, so the first call wins and later calls are ignored.
pub fn configure(session: &SessionConfig) {
    let _ = CONFIG.set(Keys {
        signing: session.signing_key.as_bytes().to_vec(),
        refresh_signing: session.refresh_signing_key.as_bytes().to_vec(),
        token_expiry_secs: session.token_expiry_secs,
        refresh_expiry_secs: session.refresh_expiry_secs,
    });
}

pub(super) fn keys() -> &'static Keys {
    CONFIG.get_or_init(|| {
        let default = SessionConfig::default();
        Keys {
            signing: default.signing_key.as_bytes().to_vec(),
            refresh_signing: default.refresh_signing_key.as_bytes().to_vec(),
            token_expiry_secs: default.token_expiry_secs,
            refresh_expiry_secs: default.refresh_expiry_secs,
        }
    })
}

impl Keys {
    pub(super) fn signing(&self) -> &[u8] {
        &self.signing
    }

    pub(super) fn refresh_signing(&self) -> &[u8] {
        &self.refresh_signing
    }
}

/// Issue a brand-new pair; every pair gets a new `tid`, the handle a later logout blacklists.
pub fn mint_token_pair(
    uid: Uuid,
    username: &str,
    vars: HashMap<String, String>,
) -> (String, String) {
    pair(
        uid,
        username,
        &Uuid::new_v4().simple().to_string(),
        Utc::now().timestamp(),
        vars,
    )
}

/// The pair behind a refresh: BaaS re-issues with the same `tid` and `iat`, so
/// a token family keeps one revocation handle across refreshes.
pub fn mint_token_pair_with(
    uid: Uuid,
    username: &str,
    tid: &str,
    iat: i64,
    vars: HashMap<String, String>,
) -> (String, String) {
    pair(uid, username, tid, iat, vars)
}

/// `iat` is the moment the family was created while both expiries count from
/// now, so a refreshed token is not born already past its `exp`.
fn pair(
    uid: Uuid,
    username: &str,
    tid: &str,
    iat: i64,
    vars: HashMap<String, String>,
) -> (String, String) {
    let k = keys();
    let now = Utc::now().timestamp();
    let claims = |exp: i64| Claims {
        tid: tid.to_owned(),
        uid: uid.to_string(),
        un: username.to_owned(),
        vars: vars.clone(),
        exp,
        iat,
    };
    let token = encode(
        &Header::default(),
        &claims(now + k.token_expiry_secs),
        &EncodingKey::from_secret(&k.signing),
    )
    .expect("claims are encodable");
    let refresh_token = encode(
        &Header::default(),
        &claims(now + k.refresh_expiry_secs),
        &EncodingKey::from_secret(&k.refresh_signing),
    )
    .expect("claims are encodable");
    (token, refresh_token)
}
