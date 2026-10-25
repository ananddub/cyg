//! Link surface helpers: the two shared database-error mappers.

use crate::error::V2Error;

/// The error a guarded update turns into: zero rows is the conflict the client
/// must handle, anything else is logged rather than described.
pub(crate) fn guarded(in_use: &str, failed: &str) -> impl Fn(sqlx::Error) -> V2Error {
    move |e| match e {
        sqlx::Error::RowNotFound => V2Error::already_exists(in_use),
        other => failure(failed, other),
    }
}

/// The Internal every link route has, with the real failure logged rather than sent.
pub(crate) fn failure(failed: &str, e: sqlx::Error) -> V2Error {
    tracing::error!("[v2] {failed}: {e}");
    V2Error::internal(failed)
}
