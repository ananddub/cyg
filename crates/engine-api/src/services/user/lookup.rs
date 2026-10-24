//! The multi-get and single-get lookups behind `GET`/`POST /v2/user` and
//! `GET /v2/user/{user_id}`.

use axum::Json;
use uuid::Uuid;

use crate::ApiState;
use crate::app::middleware::Authed;
use crate::dto::user::{GetUsersQuery, UserListResponse};
use crate::error::V2Error;
use crate::services::auth::q;
use crate::services::social::tokens;

use crate::utils::user::{public_view, push_found};

impl super::UserService {
    /// `POST /v2/user` with a JSON body — the shape a BaaS runtime or a
    /// server-to-server caller uses; it resolves through the same path.
    pub async fn get_users_body(
        &self,
        state: &ApiState,
        auth: &Authed,
        body: GetUsersQuery,
    ) -> Result<Json<UserListResponse>, V2Error> {
        self.get_users(state, auth, body).await
    }

    /// `GET /v2/user?ids=&usernames=&facebook_ids=&custom_ids=` — the
    /// `GetUsers`. Nothing requested is nothing returned, as in BaaS.
    pub async fn get_users(
        &self,
        state: &ApiState,
        _auth: &Authed,
        req: GetUsersQuery,
    ) -> Result<Json<UserListResponse>, V2Error> {
        let ids = tokens(req.ids.as_ref());
        let usernames = tokens(req.usernames.as_ref());
        let facebook_ids = tokens(req.facebook_ids.as_ref());
        let custom_ids = tokens(req.custom_ids.as_ref());
        if ids.is_empty()
            && usernames.is_empty()
            && facebook_ids.is_empty()
            && custom_ids.is_empty()
        {
            return Ok(Json(UserListResponse { users: Vec::new() }));
        }

        let queries = q(state)?;
        let mut users = Vec::new();
        for id in ids {
            let parsed = Uuid::parse_str(&id).map_err(|_| {
                V2Error::invalid_argument(format!("ID '{id}' is not a valid system ID."))
            })?;
            push_found(&mut users, queries.get_user_by_id(parsed).await)?;
        }
        for username in usernames {
            push_found(&mut users, queries.get_user_by_username(&username).await)?;
        }
        for facebook_id in facebook_ids {
            push_found(
                &mut users,
                queries.get_user_by_facebook_id(&facebook_id).await,
            )?;
        }
        for custom_id in custom_ids {
            push_found(&mut users, queries.get_user_by_custom_id(&custom_id).await)?;
        }
        Ok(Json(UserListResponse { users }))
    }

    /// `GET /v2/user/{user_id}` — one account by id or username. Not a BaaS
    /// client route; the lobby screens of the shipped games call it directly.
    pub async fn get_user(
        &self,
        state: &ApiState,
        _auth: &Authed,
        raw: &str,
    ) -> Result<Json<UserListResponse>, V2Error> {
        let queries = q(state)?;
        let user = match Uuid::parse_str(raw) {
            Ok(id) => queries.get_user_by_id(id).await,
            Err(_) => queries.get_user_by_username(raw).await,
        };
        let user = user.map_err(|e| match e {
            sqlx::Error::RowNotFound => V2Error::not_found("User account not found."),
            other => V2Error::from_sqlx(other),
        })?;
        let view =
            public_view(&user).ok_or_else(|| V2Error::not_found("User account not found."))?;
        Ok(Json(UserListResponse { users: vec![view] }))
    }
}
