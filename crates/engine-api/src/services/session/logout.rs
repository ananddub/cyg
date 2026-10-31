use axum::Json;
use axum::http::HeaderMap;

use crate::ApiState;
use crate::app::middleware::session_claims;
use crate::dto::Empty;
use crate::dto::session::LogoutRequest;
use crate::error::V2Error;
use crate::infra::jwt;

use crate::utils::session::revocation;

impl super::SessionService {
    pub async fn logout(
        &self,
        state: &ApiState,
        headers: &HeaderMap,
        req: LogoutRequest,
    ) -> Result<Json<Empty>, V2Error> {
        let claims = session_claims(headers, state)?;
        let uid = claims.uuid()?;

        let (session_exp, session_id) = revocation(
            &req.token,
            jwt::parse_session,
            uid,
            "Session token invalid.",
        )?;
        let (refresh_exp, refresh_id) = revocation(
            &req.refresh_token,
            jwt::parse_refresh,
            uid,
            "Refresh token invalid.",
        )?;

        if session_id.is_empty() && refresh_id.is_empty() {
            state.sessions.remove_all(uid);
        } else {
            state
                .sessions
                .remove(uid, session_exp, &session_id, refresh_exp, &refresh_id);
        }
        Ok(Json(Empty {}))
    }
}
