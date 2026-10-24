//! The providers whose credential belongs to someone else to vouch for.
//!
//! Each route runs the field checks, then answers through `crate::utils::vendor`,
//! which explains why the claim cannot be verified here. Device/custom ids live in [`super::local`].

use axum::Json;

use super::validate;
use crate::dto::auth::GameCenterAccount;
use crate::dto::session::SessionResponse;
use crate::error::V2Error;
use crate::utils::vendor;

/// A Facebook or Google access token.
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

/// A Game Center signature bundle, which BaaS insists on field by field.
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

/// The six Game Center fields, in the order `CheckGameCenterID` receives them.
/// Shared with the link routes, which ask for the same bundle without a username.
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

/// The HMAC-signed player info of a Facebook Instant Game.
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
    // DataLoss rather than Unauthenticated: BaaS treats an unverifiable
    // signature as its own data being wrong, and the client cannot retry out of it.
    Err(V2Error::data_loss(
        "Error extracting the Facebook Instant Game player ID or validating the signature",
    ))
}

/// The runtime-provider `Authenticate` RPC.
pub fn provider(name: &str, requested_username: &str) -> Result<Json<SessionResponse>, V2Error> {
    if name.is_empty() {
        return Err(V2Error::invalid_argument("Provider name is required."));
    }
    validate::username(requested_username)?;
    Err(vendor::no_runtime_provider(name))
}
