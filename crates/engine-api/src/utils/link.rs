use crate::db::DbError;
use crate::error::V2Error;

pub(crate) fn guarded(in_use: &str, failed: &str) -> impl Fn(DbError) -> V2Error {
    move |e| match e {
        DbError::NotFound => V2Error::already_exists(in_use),
        other => failure(failed, other),
    }
}

pub(crate) fn failure(failed: &str, e: DbError) -> V2Error {
    tracing::error!("[v2] {failed}: {e}");
    V2Error::internal(failed)
}
