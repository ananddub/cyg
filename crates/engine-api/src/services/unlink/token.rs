//! The six unlinks whose column can only be refined by a value a vendor would name.
//!
//! A non-empty credential is refused with the vendor's own verification failure (the value
//! would have to be confirmed by asking that vendor); an empty one clears on the guard alone.

use axum::Json;
use axum::http::HeaderMap;
use uuid::Uuid;

use super::bearer;
use crate::ApiState;
use crate::dto::Empty;
use crate::dto::auth;
use crate::error::V2Error;
use crate::repositories::db::Queries;
use crate::services::auth::q;
use crate::utils::unlink::{failed, refused};
use crate::utils::vendor;

/// The four columns an access token refines.
#[derive(Clone, Copy)]
enum TokenColumn {
    Apple,
    Facebook,
    Google,
    Steam,
}

impl TokenColumn {
    fn provider(self) -> &'static str {
        match self {
            TokenColumn::Apple => "Apple",
            TokenColumn::Facebook => "Facebook",
            TokenColumn::Google => "Google",
            TokenColumn::Steam => "Steam",
        }
    }

    async fn clear(self, queries: &Queries, uid: Uuid) -> Result<Uuid, sqlx::Error> {
        match self {
            TokenColumn::Apple => queries.unlink_apple(uid).await,
            TokenColumn::Facebook => queries.unlink_facebook(uid).await,
            TokenColumn::Google => queries.unlink_google(uid).await,
            TokenColumn::Steam => queries.unlink_steam(uid).await,
        }
    }
}

async fn by_token(
    state: &ApiState,
    headers: &HeaderMap,
    column: TokenColumn,
    token: &str,
) -> Result<Json<Empty>, V2Error> {
    let uid = bearer(headers, state)?;
    if !token.is_empty() {
        return Err(vendor::profile(column.provider()));
    }
    let queries = q(state)?;
    let failed = failed(column.provider());
    column
        .clear(&queries, uid)
        .await
        .map_err(refused(&failed))?;
    Ok(Json(Empty {}))
}

impl super::UnlinkService {
    /// `POST /v2/account/unlink/apple`.
    pub async fn apple(
        &self,
        state: &ApiState,
        headers: &HeaderMap,
        account: auth::TokenAccount,
    ) -> Result<Json<Empty>, V2Error> {
        by_token(state, headers, TokenColumn::Apple, &account.token).await
    }

    /// `POST /v2/account/unlink/facebook`.
    pub async fn facebook(
        &self,
        state: &ApiState,
        headers: &HeaderMap,
        account: auth::TokenAccount,
    ) -> Result<Json<Empty>, V2Error> {
        by_token(state, headers, TokenColumn::Facebook, &account.token).await
    }

    /// `POST /v2/account/unlink/google`.
    pub async fn google(
        &self,
        state: &ApiState,
        headers: &HeaderMap,
        account: auth::TokenAccount,
    ) -> Result<Json<Empty>, V2Error> {
        by_token(state, headers, TokenColumn::Google, &account.token).await
    }

    /// `POST /v2/account/unlink/steam`.
    pub async fn steam(
        &self,
        state: &ApiState,
        headers: &HeaderMap,
        account: auth::TokenAccount,
    ) -> Result<Json<Empty>, V2Error> {
        by_token(state, headers, TokenColumn::Steam, &account.token).await
    }

    /// `POST /v2/account/unlink/facebookinstantgame`. An app secret this build lacks, so a
    /// non-empty signature still ends at verification.
    pub async fn facebook_instant_game(
        &self,
        state: &ApiState,
        headers: &HeaderMap,
        account: auth::InstantGameAccount,
    ) -> Result<Json<Empty>, V2Error> {
        let uid = bearer(headers, state)?;
        if !account.signed_player_info.is_empty() {
            return Err(vendor::profile("Facebook Instant Game"));
        }
        let queries = q(state)?;
        let failed = failed("Facebook Instant Game");
        queries
            .unlink_facebook_instant(uid)
            .await
            .map_err(refused(&failed))?;
        Ok(Json(Empty {}))
    }

    /// `POST /v2/account/unlink/gamecenter`. The bundle refines only when every field is
    /// present; half a signature is neither an error nor a refinement, unlike authenticating.
    pub async fn game_center(
        &self,
        state: &ApiState,
        headers: &HeaderMap,
        account: auth::GameCenterAccount,
    ) -> Result<Json<Empty>, V2Error> {
        let uid = bearer(headers, state)?;
        let complete = !account.bundle_id.is_empty()
            && !account.player_id.is_empty()
            && !account.public_key_url.is_empty()
            && !account.salt.is_empty()
            && !account.signature.is_empty()
            && account.timestamp_seconds != 0;
        if complete {
            return Err(vendor::profile("GameCenter"));
        }
        let queries = q(state)?;
        let failed = failed("GameCenter");
        queries
            .unlink_gamecenter(uid)
            .await
            .map_err(refused(&failed))?;
        Ok(Json(Empty {}))
    }
}
