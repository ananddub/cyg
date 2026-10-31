use axum::Json;

use crate::ApiState;
use crate::app::middleware::Authed;
use crate::dto::account::AccountResponse;
use crate::error::V2Error;

impl super::AccountService {
    pub async fn get_account(
        &self,
        state: &ApiState,
        auth: &Authed,
    ) -> Result<Json<AccountResponse>, V2Error> {
        let queries = crate::services::deps::q(state).await?;
        let user = queries.get_user_by_id(auth.uid()).await?;
        let devices = queries
            .list_device_push_tokens(auth.uid())
            .await
            .unwrap_or_default();
        Ok(Json(AccountResponse::from_rows(user, devices)))
    }
}
