use auto_di::singleton;
use axum::Json;
use axum::http::HeaderMap;

use super::{local, social, validate};
use crate::ApiState;
use crate::app::middleware::server_key;
use crate::dto::auth;
use crate::dto::auth::AuthQuery;
use crate::dto::session::SessionResponse;
use crate::error::V2Error;
use crate::utils::vendor;

pub struct AuthService;

#[singleton]
impl AuthService {
    fn new() -> Self {
        Self
    }
}

fn authorize(headers: &HeaderMap, state: &ApiState, query: &AuthQuery) -> Result<bool, V2Error> {
    let create = query.create()?;
    server_key(headers, state)?;
    Ok(create)
}

impl AuthService {
    pub async fn device(
        &self,
        state: &ApiState,
        headers: &HeaderMap,
        query: &AuthQuery,
        account: auth::IdAccount,
    ) -> Result<Json<SessionResponse>, V2Error> {
        let create = authorize(headers, state, query)?;
        validate::id_field(&account.id, "Device ID", 10)?;
        let username = validate::username(&query.username)?;
        local::device(state, &account.id, &username, create, account.vars).await
    }

    pub async fn custom(
        &self,
        state: &ApiState,
        headers: &HeaderMap,
        query: &AuthQuery,
        account: auth::IdAccount,
    ) -> Result<Json<SessionResponse>, V2Error> {
        let create = authorize(headers, state, query)?;
        validate::id_field(&account.id, "Custom ID", 6)?;
        let username = validate::username(&query.username)?;
        local::custom(state, &account.id, &username, create, account.vars).await
    }

    pub async fn email(
        &self,
        state: &ApiState,
        headers: &HeaderMap,
        query: &AuthQuery,
        account: Option<auth::EmailAccount>,
    ) -> Result<Json<SessionResponse>, V2Error> {
        let create = authorize(headers, state, query)?;
        let Some(account) = account else {
            return Err(V2Error::invalid_argument(
                "Email address and password is required.",
            ));
        };
        let login = validate::email_login(&account.email, &account.password, &query.username)?;
        local::email(state, login, create, account.vars).await
    }

    pub async fn apple(
        &self,
        state: &ApiState,
        headers: &HeaderMap,
        query: &AuthQuery,
        _account: auth::TokenAccount,
    ) -> Result<Json<SessionResponse>, V2Error> {
        authorize(headers, state, query)?;
        Err(vendor::unconfigured("Apple"))
    }

    pub async fn steam(
        &self,
        state: &ApiState,
        headers: &HeaderMap,
        query: &AuthQuery,
        _account: auth::TokenAccount,
    ) -> Result<Json<SessionResponse>, V2Error> {
        authorize(headers, state, query)?;
        Err(vendor::unconfigured("Steam"))
    }

    pub async fn facebook(
        &self,
        state: &ApiState,
        headers: &HeaderMap,
        query: &AuthQuery,
        account: auth::TokenAccount,
    ) -> Result<Json<SessionResponse>, V2Error> {
        authorize(headers, state, query)?;
        social::token("Facebook", &account.token, &query.username)
    }

    pub async fn google(
        &self,
        state: &ApiState,
        headers: &HeaderMap,
        query: &AuthQuery,
        account: auth::TokenAccount,
    ) -> Result<Json<SessionResponse>, V2Error> {
        authorize(headers, state, query)?;
        social::token("Google", &account.token, &query.username)
    }

    pub async fn facebook_instant_game(
        &self,
        state: &ApiState,
        headers: &HeaderMap,
        query: &AuthQuery,
        account: auth::InstantGameAccount,
    ) -> Result<Json<SessionResponse>, V2Error> {
        authorize(headers, state, query)?;
        social::instant_game(&account.signed_player_info, &query.username)
    }

    pub async fn game_center(
        &self,
        state: &ApiState,
        headers: &HeaderMap,
        query: &AuthQuery,
        account: Option<auth::GameCenterAccount>,
    ) -> Result<Json<SessionResponse>, V2Error> {
        authorize(headers, state, query)?;
        social::game_center(account.as_ref(), &query.username)
    }

    pub async fn provider(
        &self,
        state: &ApiState,
        headers: &HeaderMap,
        query: &AuthQuery,
        account: auth::ProviderAccount,
    ) -> Result<Json<SessionResponse>, V2Error> {
        authorize(headers, state, query)?;
        social::provider(&account.provider, &query.username)
    }
}
