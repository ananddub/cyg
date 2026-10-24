//! Console statements FlashORM does not generate, kept beside
//! [`super::statements`] but split so neither file grows past a page.
//!
//! These cover the operator MFA flags and the per-user audit rollup: a granular
//! column update no generated method reaches, and a `GROUP BY` the query parser
//! cannot express as one method.

use sqlx::Row;
use uuid::Uuid;

/// Force an operator's second factor on or off. The flag only: the enrolled secret
/// and its recovery codes are untouched, so requiring MFA for someone who has it
/// enrolled does not reset them, and un-requiring does not spend the secret.
pub async fn set_mfa_required(
    pool: &sqlx::PgPool,
    id: Uuid,
    required: bool,
) -> Result<bool, sqlx::Error> {
    let done = sqlx::query(
        "UPDATE console_user SET mfa_required = $1, update_time = now() WHERE id = $2;",
    )
    .bind(required)
    .bind(id)
    .execute(pool)
    .await?;
    Ok(done.rows_affected() > 0)
}

/// Clear an operator's second factor: the secret and the recovery codes go, the
/// `mfa_required` flag stays. The enrolment is reset, not revoked — the account is
/// still expected to have one and must re-enrol at its next sign-in.
pub async fn reset_mfa_factor(pool: &sqlx::PgPool, id: Uuid) -> Result<bool, sqlx::Error> {
    let done = sqlx::query(
        "UPDATE console_user SET mfa_secret = NULL, mfa_recovery_codes = NULL, update_time = now() WHERE id = $2;",
    )
    .bind(id)
    .execute(pool)
    .await?;
    Ok(done.rows_affected() > 0)
}

/// The operators who appear in the audit trail, one entry each, most active first.
/// The console fills its username filter from this rather than paging the whole
/// trail, so a name with no entries never appears.
pub async fn audit_usernames(pool: &sqlx::PgPool) -> Result<Vec<String>, sqlx::Error> {
    let rows = sqlx::query(
        "SELECT console_username FROM console_audit_log GROUP BY console_username ORDER BY COUNT(*) DESC, console_username ASC;",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows
        .iter()
        .map(|row| row.get::<String, _>("console_username"))
        .collect())
}
