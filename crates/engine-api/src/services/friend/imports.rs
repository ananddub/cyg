use axum::Json;

use crate::dto::Empty;
use crate::error::V2Error;

use super::FriendService;

impl FriendService {
    pub async fn import_facebook_friends(&self) -> Result<Json<Empty>, V2Error> {
        Err(V2Error::failed_precondition(
            "Facebook authentication is not configured.",
        ))
    }

    pub async fn import_steam_friends(&self) -> Result<Json<Empty>, V2Error> {
        Err(V2Error::failed_precondition(
            "Steam authentication is not configured.",
        ))
    }
}
