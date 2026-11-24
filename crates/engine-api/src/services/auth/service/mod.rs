use crate::services::plane::Plane;
use auto_di::singleton;
use axum::Json;
use axum::http::HeaderMap;
use std::sync::Arc;

use super::{local, social, validate, verify};
use crate::app::middleware::server_key;
use crate::dto::auth;
use crate::dto::auth::AuthQuery;
use crate::dto::session::SessionResponse;
use crate::error::V2Error;

pub struct AuthService {
    pub(crate) plane: Arc<Plane>,
}

#[singleton]
impl AuthService {
    pub fn new(plane: Arc<Plane>) -> Self {
        Self { plane }
    }
}

fn authorize(headers: &HeaderMap, plane: &Plane, query: &AuthQuery) -> Result<bool, V2Error> {
    let create = query.create()?;
    server_key(headers, plane)?;
    Ok(create)
}

impl AuthService {
    pub async fn device(
        &self,

        headers: &HeaderMap,
        query: &AuthQuery,
        account: auth::IdAccount,
    ) -> Result<Json<SessionResponse>, V2Error> {
        let create = authorize(headers, &self.plane, query)?;
        validate::id_field(&account.id, "Device ID", 10)?;
        let username = validate::username(&query.username)?;
        local::device(&self.plane, &account.id, &username, create, account.vars).await
    }

    pub async fn custom(
        &self,

        headers: &HeaderMap,
        query: &AuthQuery,
        account: auth::IdAccount,
    ) -> Result<Json<SessionResponse>, V2Error> {
        let create = authorize(headers, &self.plane, query)?;
        validate::id_field(&account.id, "Custom ID", 6)?;
        let username = validate::username(&query.username)?;
        local::custom(&self.plane, &account.id, &username, create, account.vars).await
    }

    pub async fn email(
        &self,

        headers: &HeaderMap,
        query: &AuthQuery,
        account: Option<auth::EmailAccount>,
    ) -> Result<Json<SessionResponse>, V2Error> {
        let create = authorize(headers, &self.plane, query)?;
        let Some(account) = account else {
            return Err(V2Error::invalid_argument(
                "Email address and password is required.",
            ));
        };
        let login = validate::email_login(&account.email, &account.password, &query.username)?;
        local::email(&self.plane, login, create, account.vars).await
    }

    pub async fn apple(
        &self,

        headers: &HeaderMap,
        query: &AuthQuery,
        account: auth::TokenAccount,
    ) -> Result<Json<SessionResponse>, V2Error> {
        let create = authorize(headers, &self.plane, query)?;
        social::token(
            &self.plane,
            verify::Kind::Apple,
            &account.token,
            &query.username,
            create,
            account.vars,
        )
        .await
    }

    pub async fn steam(
        &self,

        headers: &HeaderMap,
        query: &AuthQuery,
        account: auth::TokenAccount,
    ) -> Result<Json<SessionResponse>, V2Error> {
        let create = authorize(headers, &self.plane, query)?;
        social::token(
            &self.plane,
            verify::Kind::Steam,
            &account.token,
            &query.username,
            create,
            account.vars,
        )
        .await
    }

    pub async fn facebook(
        &self,

        headers: &HeaderMap,
        query: &AuthQuery,
        account: auth::TokenAccount,
    ) -> Result<Json<SessionResponse>, V2Error> {
        let create = authorize(headers, &self.plane, query)?;
        social::token(
            &self.plane,
            verify::Kind::Facebook,
            &account.token,
            &query.username,
            create,
            account.vars,
        )
        .await
    }

    pub async fn google(
        &self,

        headers: &HeaderMap,
        query: &AuthQuery,
        account: auth::TokenAccount,
    ) -> Result<Json<SessionResponse>, V2Error> {
        let create = authorize(headers, &self.plane, query)?;
        social::token(
            &self.plane,
            verify::Kind::Google,
            &account.token,
            &query.username,
            create,
            account.vars,
        )
        .await
    }

    pub async fn facebook_instant_game(
        &self,

        headers: &HeaderMap,
        query: &AuthQuery,
        account: auth::InstantGameAccount,
    ) -> Result<Json<SessionResponse>, V2Error> {
        authorize(headers, &self.plane, query)?;
        social::instant_game(&account.signed_player_info, &query.username)
    }

    pub async fn game_center(
        &self,

        headers: &HeaderMap,
        query: &AuthQuery,
        account: Option<auth::GameCenterAccount>,
    ) -> Result<Json<SessionResponse>, V2Error> {
        authorize(headers, &self.plane, query)?;
        social::game_center(account.as_ref(), &query.username)
    }

    pub async fn provider(
        &self,

        headers: &HeaderMap,
        query: &AuthQuery,
        account: auth::ProviderAccount,
    ) -> Result<Json<SessionResponse>, V2Error> {
        let create = authorize(headers, &self.plane, query)?;
        social::provider(&self.plane, account, &query.username, create).await
    }
}
