use std::sync::Arc;

use auto_route::controller;
use axum::http::HeaderMap;
use axum::response::{IntoResponse, Response};

use crate::app::params::{Body, Params};
use crate::current_state;
use crate::dto::auth;

use crate::dto::link::SyncQuery;
use crate::services::link::LinkService;

struct LinkController {
    service: Arc<LinkService>,
}

#[controller("/v1/account/link")]
impl LinkController {
    fn new(service: Arc<LinkService>) -> Self {
        Self { service }
    }

    #[post("", response_body = crate::dto::Empty)]
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

    #[post("/apple", response_body = crate::dto::Empty)]
    async fn apple(&self, headers: HeaderMap, Body(account): Body<auth::TokenAccount>) -> Response {
        let state = current_state();
        self.service.apple(&headers, account).await.into_response()
    }

    #[post("/custom", response_body = crate::dto::Empty)]
    async fn custom(&self, headers: HeaderMap, Body(account): Body<auth::IdAccount>) -> Response {
        let state = current_state();
        self.service.custom(&headers, account).await.into_response()
    }

    #[post("/device", response_body = crate::dto::Empty)]
    async fn device(&self, headers: HeaderMap, Body(account): Body<auth::IdAccount>) -> Response {
        let state = current_state();
        self.service.device(&headers, account).await.into_response()
    }

    #[post("/email", response_body = crate::dto::Empty)]
    async fn email(&self, headers: HeaderMap, Body(account): Body<auth::EmailAccount>) -> Response {
        let state = current_state();
        self.service.email(&headers, account).await.into_response()
    }

    #[post("/facebook", response_body = crate::dto::Empty)]
    async fn facebook(
        &self,
        headers: HeaderMap,
        Params(query): Params<SyncQuery>,
        Body(account): Body<auth::TokenAccount>,
    ) -> Response {
        let state = current_state();
        self.service
            .facebook(&headers, query, account)
            .await
            .into_response()
    }

    #[post("/facebookinstantgame", response_body = crate::dto::Empty)]
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

    #[post("/gamecenter", response_body = crate::dto::Empty)]
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

    #[post("/google", response_body = crate::dto::Empty)]
    async fn google(
        &self,
        headers: HeaderMap,
        Body(account): Body<auth::TokenAccount>,
    ) -> Response {
        let state = current_state();
        self.service.google(&headers, account).await.into_response()
    }

    #[post("/steam", response_body = crate::dto::Empty)]
    async fn steam(&self, headers: HeaderMap, Body(account): Body<auth::SteamLink>) -> Response {
        let state = current_state();
        self.service.steam(&headers, account).await.into_response()
    }
}
