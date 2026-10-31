use axum::Json;
use axum::http::HeaderMap;

use super::bearer;
use crate::ApiState;
use crate::dto::Empty;
use crate::dto::auth;
use crate::dto::link::SyncQuery;
use crate::error::V2Error;
use crate::services::auth::social;
use crate::utils::vendor;

impl super::LinkService {
    pub async fn apple(
        &self,
        state: &ApiState,
        headers: &HeaderMap,
        _account: auth::TokenAccount,
    ) -> Result<Json<Empty>, V2Error> {
        bearer(headers, state)?;
        Err(vendor::unconfigured("Apple"))
    }

    pub async fn steam(
        &self,
        state: &ApiState,
        headers: &HeaderMap,
        link: auth::SteamLink,
    ) -> Result<Json<Empty>, V2Error> {
        bearer(headers, state)?;
        if link
            .account
            .as_ref()
            .is_none_or(|account| account.token.is_empty())
        {
            return Err(V2Error::invalid_argument("Steam access token is required."));
        }
        Err(vendor::unconfigured("Steam"))
    }

    pub async fn facebook(
        &self,
        state: &ApiState,
        headers: &HeaderMap,
        query: SyncQuery,
        account: auth::TokenAccount,
    ) -> Result<Json<Empty>, V2Error> {
        query.check()?;
        bearer(headers, state)?;
        if account.token.is_empty() {
            return Err(V2Error::invalid_argument(
                "Facebook access token is required.",
            ));
        }
        Err(vendor::profile("Facebook"))
    }

    pub async fn google(
        &self,
        state: &ApiState,
        headers: &HeaderMap,
        account: auth::TokenAccount,
    ) -> Result<Json<Empty>, V2Error> {
        bearer(headers, state)?;
        if account.token.is_empty() {
            return Err(V2Error::invalid_argument(
                "Google access token is required.",
            ));
        }
        Err(vendor::profile("Google"))
    }

    pub async fn facebook_instant_game(
        &self,
        state: &ApiState,
        headers: &HeaderMap,
        account: auth::InstantGameAccount,
    ) -> Result<Json<Empty>, V2Error> {
        bearer(headers, state)?;
        if account.signed_player_info.is_empty() {
            return Err(V2Error::invalid_argument(
                "Signed Player Info for a Facebook Instant Game is required.",
            ));
        }
        Err(vendor::profile("Facebook Instant Game"))
    }

    pub async fn game_center(
        &self,
        state: &ApiState,
        headers: &HeaderMap,
        account: auth::GameCenterAccount,
    ) -> Result<Json<Empty>, V2Error> {
        bearer(headers, state)?;
        social::game_center_fields(&account)?;
        Err(vendor::profile("GameCenter"))
    }

    pub async fn provider(
        &self,
        state: &ApiState,
        headers: &HeaderMap,
        account: auth::ProviderAccount,
    ) -> Result<Json<Empty>, V2Error> {
        bearer(headers, state)?;
        Err(vendor::no_runtime_provider(&account.provider))
    }
}
