use axum::Json;

use super::super::validate;
use crate::dto::session::SessionResponse;
use crate::error::V2Error;
use crate::utils::vendor;

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
