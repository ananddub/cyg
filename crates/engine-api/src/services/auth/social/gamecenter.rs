use axum::Json;

use super::super::validate;
use crate::dto::auth::GameCenterAccount;
use crate::dto::session::SessionResponse;
use crate::error::V2Error;
use crate::utils::vendor;

pub fn game_center(
    account: Option<&GameCenterAccount>,
    requested_username: &str,
) -> Result<Json<SessionResponse>, V2Error> {
    let Some(account) = account else {
        return Err(V2Error::invalid_argument(
            "GameCenter access credentials are required.",
        ));
    };
    game_center_fields(account)?;
    validate::username(requested_username)?;
    Err(vendor::profile("GameCenter"))
}

pub(crate) fn game_center_fields(account: &GameCenterAccount) -> Result<(), V2Error> {
    for (value, field) in [
        (&account.bundle_id, "bundle ID"),
        (&account.player_id, "player ID"),
        (&account.public_key_url, "public key URL"),
        (&account.salt, "salt"),
        (&account.signature, "signature"),
    ] {
        if value.is_empty() {
            return Err(V2Error::invalid_argument(format!(
                "GameCenter {field} is required."
            )));
        }
    }
    if account.timestamp_seconds == 0 {
        return Err(V2Error::invalid_argument(
            "GameCenter timestamp is required.",
        ));
    }
    Ok(())
}
