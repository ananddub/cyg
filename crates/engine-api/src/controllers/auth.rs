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

#[controller("/v2/account/authenticate")]
impl AuthController {
    fn new(service: Arc<AuthService>) -> Self {
        Self { service }
    }

    #[post("")]
    async fn provider(
        &self,
        headers: HeaderMap,
        Params(query): Params<AuthQuery>,
        Body(account): Body<auth::ProviderAccount>,
    ) -> Response {
        let state = current_state();
        self.service
            .provider(&state, &headers, &query, account)
            .await
            .into_response()
    }

    #[post("/apple")]
    async fn apple(
        &self,
        headers: HeaderMap,
        Params(query): Params<AuthQuery>,
        Body(account): Body<auth::TokenAccount>,
    ) -> Response {
        let state = current_state();
        self.service
            .apple(&state, &headers, &query, account)
            .await
            .into_response()
    }

    #[post("/custom")]
    async fn custom(
        &self,
        headers: HeaderMap,
        Params(query): Params<AuthQuery>,
        Body(account): Body<auth::IdAccount>,
    ) -> Response {
        let state = current_state();
        self.service
            .custom(&state, &headers, &query, account)
            .await
            .into_response()
    }

    #[post("/device")]
    async fn device(
        &self,
        headers: HeaderMap,
        Params(query): Params<AuthQuery>,
        Body(account): Body<auth::IdAccount>,
    ) -> Response {
        let state = current_state();
        self.service
            .device(&state, &headers, &query, account)
            .await
            .into_response()
    }

    #[post("/email")]
    async fn email(
        &self,
        headers: HeaderMap,
        Params(query): Params<AuthQuery>,
        Body(account): Body<Option<auth::EmailAccount>>,
    ) -> Response {
        let state = current_state();
        self.service
            .email(&state, &headers, &query, account)
            .await
            .into_response()
    }

    #[post("/facebook")]
    async fn facebook(
        &self,
        headers: HeaderMap,
        Params(query): Params<AuthQuery>,
        Body(account): Body<auth::TokenAccount>,
    ) -> Response {
        let state = current_state();
        self.service
            .facebook(&state, &headers, &query, account)
            .await
            .into_response()
    }

    #[post("/facebookinstantgame")]
    async fn facebook_instant_game(
        &self,
        headers: HeaderMap,
        Params(query): Params<AuthQuery>,
        Body(account): Body<auth::InstantGameAccount>,
    ) -> Response {
        let state = current_state();
        self.service
            .facebook_instant_game(&state, &headers, &query, account)
            .await
            .into_response()
    }

    #[post("/gamecenter")]
    async fn game_center(
        &self,
        headers: HeaderMap,
        Params(query): Params<AuthQuery>,
        Body(account): Body<Option<auth::GameCenterAccount>>,
    ) -> Response {
        let state = current_state();
        self.service
            .game_center(&state, &headers, &query, account)
            .await
            .into_response()
    }

    #[post("/google")]
    async fn google(
        &self,
        headers: HeaderMap,
        Params(query): Params<AuthQuery>,
        Body(account): Body<auth::TokenAccount>,
    ) -> Response {
        let state = current_state();
        self.service
            .google(&state, &headers, &query, account)
            .await
            .into_response()
    }

    #[post("/steam")]
    async fn steam(
        &self,
        headers: HeaderMap,
        Params(query): Params<AuthQuery>,
        Body(account): Body<auth::TokenAccount>,
    ) -> Response {
        let state = current_state();
        self.service
            .steam(&state, &headers, &query, account)
            .await
            .into_response()
    }
}
