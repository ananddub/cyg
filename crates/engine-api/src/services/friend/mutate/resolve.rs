use uuid::Uuid;

use crate::db::DbError;
use crate::dto::friend::TargetsQuery;
use crate::error::V2Error;
use crate::repositories::db::Queries;
use crate::services::social::{self, tokens};

pub(super) async fn resolve_targets(
    queries: &Queries,
    me: Uuid,
    my_username: &str,
    req: &TargetsQuery,
    self_message: &str,
) -> Result<Option<Vec<Uuid>>, V2Error> {
    let ids = tokens(req.ids.as_ref());
    let usernames = tokens(req.usernames.as_ref());
    if ids.is_empty() && usernames.is_empty() {
        return Ok(None);
    }

    let mut out: Vec<Uuid> = Vec::new();
    for id in ids {
        if id == social::uid(&me) {
            return Err(V2Error::invalid_argument(self_message));
        }
        let parsed = Uuid::parse_str(&id).unwrap_or(Uuid::nil());
        if parsed.is_nil() {
            return Err(V2Error::invalid_argument(format!(
                "Invalid user ID '{id}'."
            )));
        }
        out.push(parsed);
    }
    for username in usernames {
        if username == my_username {
            return Err(V2Error::invalid_argument(self_message));
        }
        match queries.get_user_by_username(&username).await {
            Ok(user) => out.push(user.id),
            Err(DbError::NotFound) => {}
            Err(e) => return Err(V2Error::from(e)),
        }
    }
    out.dedup();
    Ok(Some(out))
}

pub(super) async fn edge_state(
    queries: &Queries,
    source: Uuid,
    destination: Uuid,
) -> Result<Option<i16>, V2Error> {
    match queries.get_friend_edge_state(source, destination).await {
        Ok(state) => Ok(Some(state)),
        Err(DbError::NotFound) => Ok(None),
        Err(e) => Err(V2Error::from(e)),
    }
}
