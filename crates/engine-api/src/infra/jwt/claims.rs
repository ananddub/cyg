//! The claim set both tokens of a pair carry; the auth reply shape lives in
//! [`crate::dto::session`].

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::V2Error;

/// A signed session; the field order is the wire order and the `rename`s are the
/// short JSON names the claims travel under.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claims {
    /// Session id, shared by the session and refresh token of one pair.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub tid: String,
    /// User id (UUID text).
    pub uid: String,
    /// Username.
    #[serde(rename = "usn", default, skip_serializing_if = "String::is_empty")]
    pub un: String,
    /// The `vars` the client authenticated with, echoed back inside its token.
    #[serde(rename = "vrs", default, skip_serializing_if = "HashMap::is_empty")]
    pub vars: HashMap<String, String>,
    /// Expiry, seconds since the epoch. Required, and enforced on decode.
    pub exp: i64,
    /// Issue time, seconds since the epoch. Preserved across refreshes.
    #[serde(default, skip_serializing_if = "num_zero")]
    pub iat: i64,
}

fn num_zero(v: &i64) -> bool {
    *v == 0
}

impl Claims {
    /// The user id as a `Uuid`. A well-formed session always has one; a forged
    /// token with a garbage uid is an auth failure, not a panic.
    pub fn uuid(&self) -> Result<Uuid, V2Error> {
        Uuid::parse_str(&self.uid)
            .map_err(|_| V2Error::unauthenticated("session token carries an invalid user id"))
    }
}
