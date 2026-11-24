use std::sync::Arc;

use auto_route::controller;
use axum::http::HeaderMap;
use axum::response::{IntoResponse, Response};

use crate::app::params::{Body, Params};
use crate::current_state;

use crate::dto::auth::{self, AuthQuery};
use crate::services::auth::AuthService;

struct AuthController {
    service: Arc<AuthService>,
}

#[controller("/v1/account/authenticate")]
impl AuthController {
    fn new(service: Arc<AuthService>) -> Self {
        Self { service }
    }

    #[post("", response_body = crate::dto::session::SessionResponse)]
    async fn provider(
        &self,
        headers: HeaderMap,
        Params(query): Params<AuthQuery>,
        Body(account): Body<auth::ProviderAccount>,
    ) -> Response {
        let state = current_state();
        self.service
            .provider(&headers, &query, account)
            .await
            .into_response()
    }

    #[post("/apple", response_body = crate::dto::session::SessionResponse)]
    async fn apple(
        &self,
        headers: HeaderMap,
        Params(query): Params<AuthQuery>,
        Body(account): Body<auth::TokenAccount>,
    ) -> Response {
        let state = current_state();
        self.service
            .apple(&headers, &query, account)
            .await
            .into_response()
    }

    #[post("/custom", response_body = crate::dto::session::SessionResponse)]
    async fn custom(
        &self,
        headers: HeaderMap,
        Params(query): Params<AuthQuery>,
        Body(account): Body<auth::IdAccount>,
    ) -> Response {
        let state = current_state();
        self.service
            .custom(&headers, &query, account)
            .await
            .into_response()
    }

    #[post("/device", response_body = crate::dto::session::SessionResponse)]
    async fn device(
        &self,
        headers: HeaderMap,
        Params(query): Params<AuthQuery>,
        Body(account): Body<auth::IdAccount>,
    ) -> Response {
        let state = current_state();
        self.service
            .device(&headers, &query, account)
            .await
            .into_response()
    }

    #[post("/email", response_body = crate::dto::session::SessionResponse)]
    async fn email(
        &self,
        headers: HeaderMap,
        Params(query): Params<AuthQuery>,
        Body(account): Body<Option<auth::EmailAccount>>,
    ) -> Response {
        let state = current_state();
        self.service
            .email(&headers, &query, account)
            .await
            .into_response()
    }

    #[post("/facebook", response_body = crate::dto::session::SessionResponse)]
    async fn facebook(
        &self,
        headers: HeaderMap,
        Params(query): Params<AuthQuery>,
        Body(account): Body<auth::TokenAccount>,
    ) -> Response {
        let state = current_state();
        self.service
            .facebook(&headers, &query, account)
            .await
            .into_response()
    }

    #[post("/facebookinstantgame", response_body = crate::dto::session::SessionResponse)]
    async fn facebook_instant_game(
        &self,
        headers: HeaderMap,
        Params(query): Params<AuthQuery>,
        Body(account): Body<auth::InstantGameAccount>,
    ) -> Response {
        let state = current_state();
        self.service
            .facebook_instant_game(&headers, &query, account)
            .await
            .into_response()
    }

    #[post("/gamecenter", response_body = crate::dto::session::SessionResponse)]
    async fn game_center(
        &self,
        headers: HeaderMap,
        Params(query): Params<AuthQuery>,
        Body(account): Body<Option<auth::GameCenterAccount>>,
    ) -> Response {
        let state = current_state();
        self.service
            .game_center(&headers, &query, account)
            .await
            .into_response()
    }

    #[post("/google", response_body = crate::dto::session::SessionResponse)]
    async fn google(
        &self,
        headers: HeaderMap,
        Params(query): Params<AuthQuery>,
        Body(account): Body<auth::TokenAccount>,
    ) -> Response {
        let state = current_state();
        self.service
            .google(&headers, &query, account)
            .await
            .into_response()
    }

    #[post("/steam", response_body = crate::dto::session::SessionResponse)]
    async fn steam(
        &self,
        headers: HeaderMap,
        Params(query): Params<AuthQuery>,
        Body(account): Body<auth::TokenAccount>,
    ) -> Response {
        let state = current_state();
        self.service
            .steam(&headers, &query, account)
            .await
            .into_response()
    }
}
