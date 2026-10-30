use std::collections::HashMap;
use std::sync::OnceLock;

use chrono::Utc;
use engine_config::{Rotation, SessionConfig};
use jsonwebtoken::{EncodingKey, Header, encode};
use uuid::Uuid;

use super::claims::Claims;

static CONFIG: OnceLock<Keys> = OnceLock::new();

/// The secret set a process signs with, plus the retired set it still verifies
/// for one TTL window. `key_id` labels the set, so a token says which secret it
/// expects instead of the server guessing.
pub(super) struct Keys {
    pub(super) signing: Vec<u8>,
    pub(super) refresh_signing: Vec<u8>,
    pub(super) token_expiry_secs: i64,
    pub(super) refresh_expiry_secs: i64,
    pub(super) key_id: String,
    pub(super) previous: Option<Previous>,
}

pub(super) struct Previous {
    pub(super) key_id: String,
    pub(super) signing: Vec<u8>,
    pub(super) refresh_signing: Vec<u8>,
}

fn previous_of(rotation: &Rotation) -> Option<Previous> {
    if rotation.previous_signing_key.is_empty() && rotation.previous_refresh_signing_key.is_empty()
    {
        return None;
    }
    Some(Previous {
        key_id: rotation.previous_signing_key_id.clone(),
        signing: rotation.previous_signing_key.as_bytes().to_vec(),
        refresh_signing: rotation.previous_refresh_signing_key.as_bytes().to_vec(),
    })
}

fn build(session: &SessionConfig, rotation: &Rotation) -> Keys {
    Keys {
        signing: session.signing_key.as_bytes().to_vec(),
        refresh_signing: session.refresh_signing_key.as_bytes().to_vec(),
        token_expiry_secs: session.token_expiry_secs,
        refresh_expiry_secs: session.refresh_expiry_secs,
        key_id: rotation.signing_key_id.clone(),
        previous: previous_of(rotation),
    }
}

pub fn configure(session: &SessionConfig) {
    let _ = CONFIG.set(build(session, Rotation::global()));
}

pub(super) fn keys() -> &'static Keys {
    CONFIG.get_or_init(|| build(&SessionConfig::default(), Rotation::global()))
}

impl Keys {
    /// The header this process signs with. `kid` is what lets a retired key be
    /// dropped without invalidating tokens that were minted while it was live.
    pub(super) fn header(&self) -> Header {
        Header {
            kid: Some(self.key_id.clone()),
            ..Default::default()
        }
    }

    /// The secret a token's `kid` names. No `kid` means the token predates
    /// labelling, so the current key is the only honest answer; a `kid` this
    /// process was not given a secret for is refused rather than guessed at.
    pub(super) fn secret_for(&self, kid: Option<&str>, refresh: bool) -> Option<&[u8]> {
        let current = kid.is_none() || kid == Some(self.key_id.as_str());
        let (signing, refresh_signing) = if current {
            (&self.signing, &self.refresh_signing)
        } else if let Some(previous) = self
            .previous
            .as_ref()
            .filter(|p| kid == Some(p.key_id.as_str()))
        {
            (&previous.signing, &previous.refresh_signing)
        } else {
            return None;
        };
        let secret = if refresh { refresh_signing } else { signing };
        if secret.is_empty() {
            None
        } else {
            Some(secret.as_slice())
        }
    }
}

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

pub fn mint_token_pair_with(
    uid: Uuid,
    username: &str,
    tid: &str,
    iat: i64,
    vars: HashMap<String, String>,
) -> (String, String) {
    pair(uid, username, tid, iat, vars)
}

fn pair(
    uid: Uuid,
    username: &str,
    tid: &str,
    iat: i64,
    vars: HashMap<String, String>,
) -> (String, String) {
    pair_with(keys(), uid, username, tid, iat, vars)
}

fn pair_with(
    k: &Keys,
    uid: Uuid,
    username: &str,
    tid: &str,
    iat: i64,
    vars: HashMap<String, String>,
) -> (String, String) {
    let now = Utc::now().timestamp();
    let claims = |exp: i64| Claims {
        tid: tid.to_owned(),
        uid: uid.to_string(),
        un: username.to_owned(),
        vars: vars.clone(),
        exp,
        iat,
    };
    let header = k.header();
    let token = encode(
        &header,
        &claims(now + k.token_expiry_secs),
        &EncodingKey::from_secret(&k.signing),
    )
    .expect("claims are encodable");
    let refresh_token = encode(
        &header,
        &claims(now + k.refresh_expiry_secs),
        &EncodingKey::from_secret(&k.refresh_signing),
    )
    .expect("claims are encodable");
    (token, refresh_token)
}

/// Mint with an explicit key set. Only tests use it; production mints with the
/// configured keys, because a token must never be signed by a key the process
/// has not published a `kid` for.
#[cfg(test)]
pub(super) fn mint_pair_with(
    keys: &Keys,
    uid: Uuid,
    username: &str,
    vars: HashMap<String, String>,
) -> (String, String) {
    pair_with(
        keys,
        uid,
        username,
        &Uuid::new_v4().simple().to_string(),
        Utc::now().timestamp(),
        vars,
    )
}
