use axum::Json;

use crate::ApiState;
use crate::app::middleware::Authed;
use crate::dto::Empty;
use crate::error::V2Error;

impl super::AccountService {
    pub async fn delete_account(
        &self,
        state: &ApiState,
        auth: &Authed,
    ) -> Result<Json<Empty>, V2Error> {
        let queries = crate::services::deps::q(state).await?;
        queries.create_tombstone(auth.uid()).await?;
        queries.delete_account(auth.uid()).await?;
        Ok(Json(Empty {}))
    }
}
