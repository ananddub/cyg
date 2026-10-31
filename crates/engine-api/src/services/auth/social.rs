use axum::Json;

use super::validate;
use crate::dto::auth::GameCenterAccount;
use crate::dto::session::SessionResponse;
use crate::error::V2Error;
use crate::utils::vendor;

pub fn token(
    profile: &str,
    raw: &str,
    requested_username: &str,
) -> Result<Json<SessionResponse>, V2Error> {
    if raw.is_empty() {
        return Err(V2Error::invalid_argument(format!(
            "{profile} access token is required."
        )));
    }
    validate::username(requested_username)?;
    Err(vendor::profile(profile))
}

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

pub fn instant_game(
    signed_player_info: &str,
    requested_username: &str,
) -> Result<Json<SessionResponse>, V2Error> {
    if signed_player_info.is_empty() {
        return Err(V2Error::invalid_argument(
            "Signed Player Info for a Facebook Instant Game is required.",
        ));
    }
    validate::username(requested_username)?;
    Err(V2Error::data_loss(
        "Error extracting the Facebook Instant Game player ID or validating the signature",
    ))
}

pub fn provider(name: &str, requested_username: &str) -> Result<Json<SessionResponse>, V2Error> {
    if name.is_empty() {
        return Err(V2Error::invalid_argument("Provider name is required."));
    }
    validate::username(requested_username)?;
    Err(vendor::no_runtime_provider(name))
}
