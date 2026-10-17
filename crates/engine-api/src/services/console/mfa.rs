//! `/v2/console/mfa` — the signed-in operator's own second factor, and the check
//! sign-in performs against it.
//!
//! The secret is stored base32 as RFC 4648 spells it; the recovery codes are stored
//! as their SHA-256 hashes, so reading the table cannot hand out a working code.

use axum::Json;
use chrono::Utc;

use crate::app::middleware::ConsoleAuthed;
use crate::dto::Empty;
use crate::dto::console::auth::{MfaCodeRequest, MfaEnrolmentResponse};
use crate::error::V2Error;
use crate::repositories::console_mutations::{FindConsoleUserMfaRow, SetConsoleUserMfaParams};
use crate::repositories::db::Queries;
use crate::utils::totp;
use crate::ApiState;

use super::RECOVERY_CODES;

/// The wording a login answers with when a code is required. The console matches on
/// it to show the code field instead of blaming the password.
pub(super) const MFA_CODE_REQUIRED: &str = "MFA code required.";

impl super::ConsoleService {
    /// The second-factor check sign-in performs: a digit code, or one of the codes
    /// written down at enrolment.
    pub(super) async fn verify_second_factor(
        &self,
        queries: &Queries,
        row: &FindConsoleUserMfaRow,
        code: &str,
    ) -> Result<(), V2Error> {
        let Some(secret) = row.mfa_secret.as_deref().map(String::from_utf8_lossy) else {
            if row.mfa_required.unwrap_or(false) {
                // Marked as requiring MFA with nothing enrolled: letting this
                // through would be exactly the bypass the flag forbids.
                return Err(V2Error::permission_denied(
                    "MFA is required for this console user but has not been enrolled.",
                ));
            }
            return Ok(());
        };

        let code = code.trim();
        if code.is_empty() {
            return Err(needs_code());
        }
        if totp::verify(&secret, code, Utc::now().timestamp()) {
            return Ok(());
        }
        if consume_recovery_code(queries, row, code).await? {
            return Ok(());
        }
        Err(V2Error::unauthenticated("MFA code invalid."))
    }

    /// Begin enrolment: mint a secret and the recovery codes, and store both. The
    /// codes are returned here and never again — only their hashes are kept.
    pub async fn enrol_mfa(
        &self,
        state: &ApiState,
        claims: &ConsoleAuthed,
    ) -> Result<Json<MfaEnrolmentResponse>, V2Error> {
        let queries = super::queries(state)?;
        let row = self.console_row(&queries, claims).await?;

        let secret = totp::generate_secret();
        let codes = totp::generate_recovery_codes(RECOVERY_CODES);
        let hashes: Vec<String> = codes
            .iter()
            .map(|code| totp::hash_recovery_code(code))
            .collect();
        let stored = serde_json::to_vec(&hashes).expect("recovery-code hashes are encodable");

        queries
            .set_console_user_mfa(&SetConsoleUserMfaParams {
                mfa_secret: secret.as_bytes().to_vec(),
                mfa_recovery_codes: stored,
                mfa_required: true,
                id: row.id,
            })
            .await?;
        super::audit(
            state,
            claims,
            "mfa.enrol",
            "console",
            Some(row.id),
            "Started MFA enrolment",
        )
        .await;

        Ok(Json(MfaEnrolmentResponse {
            uri: totp::otpauth_uri(&secret, "OpenOxide Console", &row.username),
            secret,
            recovery_codes: codes,
        }))
    }

    /// Confirm the code the authenticator is now showing. Nothing changes if it does
    /// not match, so a mistyped digit is a retry rather than a lockout.
    pub async fn confirm_mfa(
        &self,
        state: &ApiState,
        claims: &ConsoleAuthed,
        body: MfaCodeRequest,
    ) -> Result<Json<Empty>, V2Error> {
        let queries = super::queries(state)?;
        let row = self.console_row(&queries, claims).await?;
        let Some(secret) = row.mfa_secret.as_deref().map(String::from_utf8_lossy) else {
            return Err(V2Error::failed_precondition("MFA has not been enrolled."));
        };
        if !totp::verify(&secret, body.code.trim(), Utc::now().timestamp()) {
            return Err(V2Error::unauthenticated("MFA code invalid."));
        }
        super::audit(
            state,
            claims,
            "mfa.confirm",
            "console",
            Some(row.id),
            "Confirmed MFA enrolment",
        )
        .await;
        Ok(Json(Empty {}))
    }

    /// Turn the second factor off. A code is required: the point of MFA is that a
    /// stolen console token alone cannot remove it.
    pub async fn disable_mfa(
        &self,
        state: &ApiState,
        claims: &ConsoleAuthed,
        body: MfaCodeRequest,
    ) -> Result<Json<Empty>, V2Error> {
        let queries = super::queries(state)?;
        let row = self.console_row(&queries, claims).await?;
        self.verify_second_factor(&queries, &row, &body.code).await?;
        queries.clear_console_user_mfa(row.id).await?;
        super::audit(
            state,
            claims,
            "mfa.disable",
            "console",
            Some(row.id),
            "Disabled MFA",
        )
        .await;
        Ok(Json(Empty {}))
    }

    /// The signed-in operator's own row. The config bootstrap superadmin has none,
    /// so it cannot enrol: it is the account of last resort and stays password-only.
    async fn console_row(
        &self,
        queries: &Queries,
        claims: &ConsoleAuthed,
    ) -> Result<FindConsoleUserMfaRow, V2Error> {
        let Some(id) = claims.0.uuid() else {
            return Err(V2Error::failed_precondition(
                "The bootstrap console account cannot enrol MFA.",
            ));
        };
        queries
            .find_console_user_mfa_by_id(id)
            .await
            .map_err(|_| V2Error::not_found("Console user not found."))
    }
}

fn needs_code() -> V2Error {
    V2Error::unauthenticated(MFA_CODE_REQUIRED)
}

/// Spend one recovery code: it must hash to one of the stored values, and it is
/// removed so it cannot be replayed. A stored list that cannot be parsed counts as
/// no codes at all, which refuses rather than admits.
async fn consume_recovery_code(
    queries: &Queries,
    row: &FindConsoleUserMfaRow,
    code: &str,
) -> Result<bool, V2Error> {
    let Some(raw) = row.mfa_recovery_codes.as_deref() else {
        return Ok(false);
    };
    let Ok(mut codes) = serde_json::from_slice::<Vec<String>>(raw) else {
        return Ok(false);
    };
    let presented = totp::hash_recovery_code(code);
    let Some(index) = codes.iter().position(|stored| stored == &presented) else {
        return Ok(false);
    };
    codes.remove(index);
    let encoded = serde_json::to_vec(&codes).expect("a recovery-code list is encodable");
    queries
        .set_console_user_recovery_codes(&encoded, row.id)
        .await?;
    Ok(true)
}
