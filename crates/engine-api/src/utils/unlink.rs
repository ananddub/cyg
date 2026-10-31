use crate::db::DbError;
use crate::error::V2Error;

pub(crate) fn failed(provider: &str) -> String {
    format!("Error while trying to unlink {provider} ID.")
}

pub(crate) fn last_account() -> V2Error {
    V2Error::permission_denied(
        "Cannot unlink last account identifier. Check profile exists and is not last link.",
    )
}

pub(crate) fn refused(failed: &str) -> impl Fn(DbError) -> V2Error + '_ {
    move |e| match e {
        DbError::NotFound => last_account(),
        other => {
            tracing::error!("[v2] {failed}: {other}");
            V2Error::internal(failed)
        }
    }
}
