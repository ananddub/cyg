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
        let uid = auth.uid();
        let queries = crate::services::deps::q(state).await?;
        queries.create_tombstone(uid).await?;
        queries.delete_account(uid).await?;
        crate::services::webhook::emit(
            state,
            "account.deleted",
            &uid.to_string(),
            serde_json::json!({ "user_id": uid, "method": "self" }),
        )
        .await;
        Ok(Json(Empty {}))
    }
}
