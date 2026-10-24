//! `POST /v2/account/session/refresh` — server key in, fresh token pair out.

use axum::Json;
use axum::http::HeaderMap;

use crate::ApiState;
use crate::app::middleware::server_key;
use crate::dto::session::{RefreshRequest, SessionResponse};
use crate::error::V2Error;
use crate::infra::jwt;
use crate::services::auth::q;
use crate::utils::auth as rows;

impl super::SessionService {
    pub async fn refresh(
        &self,
        state: &ApiState,
        headers: &HeaderMap,
        req: RefreshRequest,
    ) -> Result<Json<SessionResponse>, V2Error> {
        server_key(headers, state)?;
        if req.token.is_empty() {
            return Err(V2Error::invalid_argument("Refresh token is required."));
        }
        let claims = jwt::verify_refresh(&req.token)?;
        let uid = claims.uuid()?;
        if !state
            .sessions
            .is_valid_refresh(uid, claims.exp, &claims.tid)
        {
            return Err(V2Error::unauthenticated(
                "Refresh token invalid or expired.",
            ));
        }

        let user = match q(state)?.get_user_by_id(uid).await {
            Ok(user) => rows::existing(user)?,
            Err(sqlx::Error::RowNotFound) => {
                return Err(V2Error::not_found("User account not found."));
            }
            Err(e) => return Err(rows::lookup_failed(e)),
        };

        // BaaS re-issues the family under the same `tid` and `iat`, so one
        // blacklisted id still covers every token this session lineage mints.
        let vars = req.vars.unwrap_or(claims.vars);
        let (token, refresh_token) =
            jwt::mint_token_pair_with(user.id, &user.username, &claims.tid, claims.iat, vars);
        Ok(Json(SessionResponse {
            created: false,
            token,
            refresh_token,
        }))
    }
}
