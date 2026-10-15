//! Auto-registered bindings for `GET`, `PUT` and `DELETE /v2/account`.
//!
//! Methods return `Response` (not `Json<T>`) because `auto_route` would otherwise
//! require `T: poem_openapi::Type`; plane state comes from `current_state` as the
//! auto-registered router is stateless.

use std::sync::Arc;

use auto_route::controller;
use axum::extract::Json;
use axum::response::{IntoResponse, Response};

use crate::current_state;
use crate::app::middleware::Authed;
use crate::app::params::Params;

use crate::dto::account::{GetAccountQuery, UpdateAccountRequest};
use crate::services::account::AccountService;

/// The `PUT /v2/account` body. `auto_route` emits an OpenAPI schema for a literal
/// `Json<T>` argument, so the plain-`serde` body is taken through this alias.
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
}
