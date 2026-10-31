use uuid::Uuid;

use crate::db::DbError;
use crate::dto::group::UsersQuery;
use crate::error::V2Error;
use crate::repositories::db::Queries;
use crate::services::social::tokens;

pub(super) async fn resolve_users(
    queries: &Queries,
    raw: &UsersQuery,
) -> Result<Vec<Uuid>, V2Error> {
    let mut ids = Vec::new();
    for token in tokens(raw.user_ids.as_ref()) {
        let parsed = Uuid::parse_str(&token)
            .ok()
            .filter(|id| !id.is_nil())
            .ok_or_else(|| V2Error::invalid_argument("User ID must be a valid ID."))?;
        queries.get_user_by_id(parsed).await.map_err(|e| match e {
            DbError::NotFound => V2Error::invalid_argument("One or more users not found."),
            other => V2Error::from(other),
        })?;
        if !ids.contains(&parsed) {
            ids.push(parsed);
        }
    }
    Ok(ids)
}
