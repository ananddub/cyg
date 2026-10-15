//! Member-id resolution shared by the mutating membership verbs.

use uuid::Uuid;

use crate::dto::group::UsersQuery;
use crate::error::V2Error;
use crate::repositories::db::Queries;
use crate::services::social::tokens;

/// BaaS resolves member ids only — an unknown id fails the whole call with
/// `"One or more users not found."` rather than being dropped.
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
            sqlx::Error::RowNotFound => V2Error::invalid_argument("One or more users not found."),
            other => V2Error::from_sqlx(other),
        })?;
        if !ids.contains(&parsed) {
            ids.push(parsed);
        }
    }
    Ok(ids)
}
