//! Decoding a token presented by a client. Two entry shapes exist because the API
//! treats them differently: the middleware wants a verdict (`Unauthenticated`),
//! while `SessionLogout` answers a bad token with `InvalidArgument`.

use jsonwebtoken::{DecodingKey, Validation};
use uuid::Uuid;

use super::claims::Claims;
use super::token::keys;
use crate::error::V2Error;

/// Verify a session token; a refresh token is signed with a different key, so it
/// fails here rather than needing a marker inside the claims.
pub fn verify_session(token: &str) -> Result<Claims, V2Error> {
    parse_session(token).ok_or_else(|| V2Error::unauthenticated("Auth token invalid"))
}

/// Verify a refresh token. The message is the `SessionRefresh` wording, which
/// a client's retry logic keys off.
pub fn verify_refresh(token: &str) -> Result<Claims, V2Error> {
    parse_refresh(token)
        .ok_or_else(|| V2Error::unauthenticated("Refresh token invalid or expired."))
}

/// Decode with the session key, or nothing.
pub fn parse_session(token: &str) -> Option<Claims> {
    decode_with(token, keys().signing())
}

/// Decode with the refresh key, or nothing.
pub fn parse_refresh(token: &str) -> Option<Claims> {
    decode_with(token, keys().refresh_signing())
}

/// `exp` is enforced by the validator and the user id must be a UUID, making a
/// garbage `uid` an auth failure at parse time rather than at the first query.
fn decode_with(token: &str, key: &[u8]) -> Option<Claims> {
    let claims = jsonwebtoken::decode::<Claims>(
        token,
        &DecodingKey::from_secret(key),
        &Validation::default(),
    )
    .ok()?
    .claims;
    Uuid::parse_str(&claims.uid).ok()?;
    Some(claims)
}
