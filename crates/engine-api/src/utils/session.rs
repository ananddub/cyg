use uuid::Uuid;

use crate::error::V2Error;
use crate::infra::jwt::Claims;

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
