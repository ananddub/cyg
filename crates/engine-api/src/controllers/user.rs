use std::sync::Arc;

use auto_route::controller;
use axum::extract::Path;
use axum::response::{IntoResponse, Response};

use crate::app::middleware::Authed;
use crate::app::params::{Body, Params};
use crate::current_state;

use crate::dto::user::GetUsersQuery;
use crate::services::user::UserService;

struct UserController {
    service: Arc<UserService>,
}

#[controller("/v2/user")]
impl UserController {
    fn new(service: Arc<UserService>) -> Self {
        Self { service }
    }

    #[get("")]
    async fn get_users(&self, auth: Authed, Params(req): Params<GetUsersQuery>) -> Response {
        let state = current_state();
        self.service
            .get_users(&state, &auth, req)
            .await
            .into_response()
    }

    #[post("")]
    async fn get_users_body(&self, auth: Authed, Body(body): Body<GetUsersQuery>) -> Response {
        let state = current_state();
        self.service
            .get_users_body(&state, &auth, body)
            .await
            .into_response()
    }

    #[get("/{user_id}")]
    async fn get_user(&self, auth: Authed, Path(raw): Path<String>) -> Response {
        let state = current_state();
        self.service
            .get_user(&state, &auth, &raw)
            .await
            .into_response()
    }
}
