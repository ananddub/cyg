//! Resolving the shared `?ids=&usernames=` request shape into target IDs.

use uuid::Uuid;

use crate::dto::friend::TargetsQuery;
use crate::error::V2Error;
use crate::repositories::db::Queries;
use crate::services::social::{self, tokens};

/// `None` is the "nothing was asked for, answer empty success"; an empty `Vec` is
/// "something was asked for but nothing resolved", which each verb reports differently.
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
            // An unknown username is dropped, not an error: BaaS resolves the
            // list first and only rejects it when nothing at all was valid.
            Ok(user) => out.push(user.id),
            Err(sqlx::Error::RowNotFound) => {}
            Err(e) => return Err(V2Error::from_sqlx(e)),
        }
    }
    out.dedup();
    Ok(Some(out))
}

/// the `addFriend` accept predicate is `(me→them, 2) OR (them→me, 1)`. The
/// generated [`accept_friend_invite`] is symmetric, which is only safe behind this check.
pub(super) async fn edge_state(
    queries: &Queries,
    source: Uuid,
    destination: Uuid,
) -> Result<Option<i16>, V2Error> {
    match queries.get_friend_edge_state(source, destination).await {
        Ok(state) => Ok(Some(state)),
        Err(sqlx::Error::RowNotFound) => Ok(None),
        Err(e) => Err(V2Error::from_sqlx(e)),
    }
}
