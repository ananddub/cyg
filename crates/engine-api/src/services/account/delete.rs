//! `DELETE /v2/account` — the tombstone, then the row.

use axum::Json;

use crate::dto::Empty;
use crate::error::V2Error;
use crate::app::middleware::Authed;
use crate::ApiState;

impl super::AccountService {
    /// The `user_tombstone` is written first, so a later credential for an identifier
    /// this account held is refused instead of silently creating a new account.
    pub async fn delete_account(
        &self,
        state: &ApiState,
        auth: &Authed,
    ) -> Result<Json<Empty>, V2Error> {
        let queries = state.q.clone().ok_or_else(V2Error::no_db)?;
        queries.create_tombstone(auth.uid()).await?;
        queries.delete_account(auth.uid()).await?;
        Ok(Json(Empty {}))
    }
}
