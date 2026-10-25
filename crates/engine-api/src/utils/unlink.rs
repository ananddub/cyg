//! Unlink helpers: the provider-named failure and the guard-refusal mapper.

use crate::error::V2Error;

/// Every column unlink answers a database fault with this, named for the provider.
pub(crate) fn failed(provider: &str) -> String {
    format!("Error while trying to unlink {provider} ID.")
}

/// The refusal every guarded unlink shares. Its wording answers two cases: the value
/// was not the one the account holds, or clearing it would lock the account out.
pub(crate) fn last_account() -> V2Error {
    V2Error::permission_denied(
        "Cannot unlink last account identifier. Check profile exists and is not last link.",
    )
}

/// A guarded `UPDATE`/`DELETE` that matched nothing: on this surface the refusal
/// above rather than a missing row, so the zero-row case never reaches
/// [`V2Error::from_sqlx`]'s `NotFound`. Anything else is logged, not sent.
pub(crate) fn refused(failed: &str) -> impl Fn(sqlx::Error) -> V2Error + '_ {
    move |e| match e {
        sqlx::Error::RowNotFound => last_account(),
        other => {
            tracing::error!("[v2] {failed}: {other}");
            V2Error::internal(failed)
        }
    }
}
