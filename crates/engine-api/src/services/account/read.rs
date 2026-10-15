//! `GET /v2/account` — the row, its links and its devices.

use axum::Json;

use crate::dto::account::AccountResponse;
use crate::error::V2Error;
use crate::app::middleware::Authed;
use crate::ApiState;

impl super::AccountService {
    pub async fn get_account(
        &self,
        state: &ApiState,
        auth: &Authed,
    ) -> Result<Json<AccountResponse>, V2Error> {
        let queries = state.q.clone().ok_or_else(V2Error::no_db)?;
        let user = queries.get_user_by_id(auth.uid()).await?;
        // The device list decorates the account, so a failure to read it is not a failure
        // to read the account: an empty list is returned rather than an error.
        let devices = queries.list_device_push_tokens(auth.uid()).await.unwrap_or_default();
        Ok(Json(AccountResponse::from_rows(user, devices)))
    }
}
