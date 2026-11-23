use std::sync::Arc;

use auto_route::controller;
use axum::http::HeaderMap;
use axum::response::{IntoResponse, Response};

use crate::app::params::Body;
use crate::current_state;
use crate::dto::auth;

use crate::services::unlink::UnlinkService;

struct UnlinkController {
    service: Arc<UnlinkService>,
}

#[controller("/v1/account/unlink")]
impl UnlinkController {
    fn new(service: Arc<UnlinkService>) -> Self {
        Self { service }
    }

    #[post("")]
    async fn provider(
        &self,
        headers: HeaderMap,
        Body(account): Body<auth::ProviderAccount>,
    ) -> Response {
        let state = current_state();
        self.service
            .provider(&headers, account)
            .await
            .into_response()
    }

    #[post("/apple")]
    async fn apple(&self, headers: HeaderMap, Body(account): Body<auth::TokenAccount>) -> Response {
        let state = current_state();
        self.service
            .apple(&headers, account)
            .await
            .into_response()
    }

    #[post("/custom")]
    async fn custom(&self, headers: HeaderMap, Body(account): Body<auth::IdAccount>) -> Response {
        let state = current_state();
        self.service
            .custom(&headers, account)
            .await
            .into_response()
    }

    #[post("/device")]
    async fn device(&self, headers: HeaderMap, Body(account): Body<auth::IdAccount>) -> Response {
        let state = current_state();
        self.service
            .device(&headers, account)
            .await
            .into_response()
    }

    #[post("/email")]
    async fn email(&self, headers: HeaderMap, Body(account): Body<auth::EmailAccount>) -> Response {
        let state = current_state();
        self.service
            .email(&headers, account)
            .await
            .into_response()
    }

    #[post("/facebook")]
    async fn facebook(
        &self,
        headers: HeaderMap,
        Body(account): Body<auth::TokenAccount>,
    ) -> Response {
        let state = current_state();
        self.service
            .facebook(&headers, account)
            .await
            .into_response()
    }

    #[post("/facebookinstantgame")]
    async fn facebook_instant_game(
        &self,
        headers: HeaderMap,
        Body(account): Body<auth::InstantGameAccount>,
    ) -> Response {
        let state = current_state();
        self.service
            .facebook_instant_game(&headers, account)
            .await
            .into_response()
    }

    #[post("/gamecenter")]
    async fn game_center(
        &self,
        headers: HeaderMap,
        Body(account): Body<auth::GameCenterAccount>,
    ) -> Response {
        let state = current_state();
        self.service
            .game_center(&headers, account)
            .await
            .into_response()
    }

    #[post("/google")]
    async fn google(
        &self,
        headers: HeaderMap,
        Body(account): Body<auth::TokenAccount>,
    ) -> Response {
        let state = current_state();
        self.service
            .google(&headers, account)
            .await
            .into_response()
    }

    #[post("/steam")]
    async fn steam(&self, headers: HeaderMap, Body(account): Body<auth::TokenAccount>) -> Response {
        let state = current_state();
        self.service
            .steam(&headers, account)
            .await
            .into_response()
    }
}
