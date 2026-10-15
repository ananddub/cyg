//! Auto-registered bindings for `/v2/friend*`.

use std::sync::Arc;

use auto_route::controller;
use axum::response::{IntoResponse, Response};

use crate::current_state;
use crate::app::middleware::Authed;
use crate::app::params::Params;

use crate::dto::friend::{FoFQuery, ListQuery, TargetsQuery};
use crate::services::friend::FriendService;

struct FriendController {
    service: Arc<FriendService>,
}

#[controller("/v2/friend")]
impl FriendController {
    fn new(service: Arc<FriendService>) -> Self {
        Self { service }
    }

    #[get("")]
    async fn list_friends(
        &self,
        auth: Authed,
        Params(req): Params<ListQuery>,
    ) -> Response {
        let state = current_state();
        self.service.list_friends(&state, &auth, req).await.into_response()
    }

    #[post("")]
    async fn add_friends(
        &self,
        auth: Authed,
        Params(req): Params<TargetsQuery>,
    ) -> Response {
        let state = current_state();
        self.service.add_friends(&state, &auth, req).await.into_response()
    }

    #[delete("")]
    async fn delete_friends(
        &self,
        auth: Authed,
        Params(req): Params<TargetsQuery>,
    ) -> Response {
        let state = current_state();
        self.service.delete_friends(&state, &auth, req).await.into_response()
    }

    #[post("/block")]
    async fn block_friends(
        &self,
        auth: Authed,
        Params(req): Params<TargetsQuery>,
    ) -> Response {
        let state = current_state();
        self.service.block_friends(&state, &auth, req).await.into_response()
    }

    #[get("/friends")]
    async fn list_friends_of_friends(
        &self,
        auth: Authed,
        Params(req): Params<FoFQuery>,
    ) -> Response {
        let state = current_state();
        self.service
            .list_friends_of_friends(&state, &auth, req)
            .await
            .into_response()
    }

    #[post("/facebook")]
    async fn import_facebook_friends(&self) -> Response {
        self.service.import_facebook_friends().await.into_response()
    }

    #[post("/steam")]
    async fn import_steam_friends(&self) -> Response {
        self.service.import_steam_friends().await.into_response()
    }
}
