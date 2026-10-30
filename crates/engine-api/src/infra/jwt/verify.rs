use jsonwebtoken::DecodingKey;
use jsonwebtoken::Validation;
use uuid::Uuid;

use super::claims::Claims;
use super::token::Keys;
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
    parse(token, keys(), false)
}

pub fn parse_refresh(token: &str) -> Option<Claims> {
    parse(token, keys(), true)
}

pub(super) fn parse(token: &str, keys: &Keys, refresh: bool) -> Option<Claims> {
    let kid = jsonwebtoken::decode_header(token).ok()?.kid;
    decode_with(token, keys.secret_for(kid.as_deref(), refresh)?)
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
