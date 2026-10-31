use std::sync::Arc;

use auto_route::controller;
use axum::extract::Json;
use axum::response::{IntoResponse, Response};

use crate::app::middleware::Authed;
use crate::app::params::{Body, Params};
use crate::current_state;

use crate::dto::account::{GetAccountQuery, UpdateAccountRequest, UpdatePlayerWalletRequest};
use crate::services::account::AccountService;

type JsonBody<T> = axum::extract::Json<T>;

struct AccountController {
    service: Arc<AccountService>,
}

#[controller("/v2/account")]
impl AccountController {
    fn new(service: Arc<AccountService>) -> Self {
        Self { service }
    }

    #[get("")]
    async fn account(&self, auth: Authed) -> Response {
        let state = current_state();
        self.service
            .get_account(&state, &auth)
            .await
            .into_response()
    }

    #[put("")]
    async fn update(
        &self,
        auth: Authed,
        Params(params): Params<GetAccountQuery>,
        Json(body): JsonBody<UpdateAccountRequest>,
    ) -> Response {
        let state = current_state();
        self.service
            .update_account(&state, &auth, params, body)
            .await
            .into_response()
    }

    #[delete("")]
    async fn delete(&self, auth: Authed) -> Response {
        let state = current_state();
        self.service
            .delete_account(&state, &auth)
            .await
            .into_response()
    }

    #[get("/wallet")]
    async fn wallet(&self, auth: Authed) -> Response {
        let state = current_state();
        self.service.get_wallet(&state, &auth).await.into_response()
    }

    #[post("/wallet")]
    async fn update_wallet(
        &self,
        auth: Authed,
        Body(body): Body<UpdatePlayerWalletRequest>,
    ) -> Response {
        let state = current_state();
        self.service
            .update_wallet(&state, &auth, body)
            .await
            .into_response()
    }

    #[put("/wallet")]
    async fn put_wallet(
        &self,
        auth: Authed,
        Body(body): Body<UpdatePlayerWalletRequest>,
    ) -> Response {
        let state = current_state();
        self.service
            .update_wallet(&state, &auth, body)
            .await
            .into_response()
    }
}
