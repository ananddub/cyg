//! Session surface helpers.

use uuid::Uuid;

use crate::error::V2Error;
use crate::infra::jwt::Claims;

/// The `tid` and `exp` to blacklist for one half of the pair. An un-named half is
/// not an error, and [`crate::infra::session_cache::SessionCache::remove`] skips an
/// empty id, so a logout naming only the refresh token leaves the session token alone.
pub(crate) fn revocation(
    raw: &str,
    parse: fn(&str) -> Option<Claims>,
    uid: Uuid,
    invalid: &str,
) -> Result<(i64, String), V2Error> {
    if raw.is_empty() {
        return Ok((0, String::new()));
    }
    let claims = parse(raw)
        .filter(|c| Uuid::parse_str(&c.uid).ok() == Some(uid))
        .ok_or_else(|| V2Error::invalid_argument(invalid.to_owned()))?;
    Ok((claims.exp, claims.tid))
}
