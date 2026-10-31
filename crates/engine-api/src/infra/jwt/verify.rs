use jsonwebtoken::{DecodingKey, Validation};
use uuid::Uuid;

use super::claims::Claims;
use super::token::keys;
use crate::error::V2Error;

pub fn verify_session(token: &str) -> Result<Claims, V2Error> {
    parse_session(token).ok_or_else(|| V2Error::unauthenticated("Auth token invalid"))
}

pub fn verify_refresh(token: &str) -> Result<Claims, V2Error> {
    parse_refresh(token)
        .ok_or_else(|| V2Error::unauthenticated("Refresh token invalid or expired."))
}

pub fn parse_session(token: &str) -> Option<Claims> {
    decode_with(token, keys().signing())
}

pub fn parse_refresh(token: &str) -> Option<Claims> {
    decode_with(token, keys().refresh_signing())
}

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
