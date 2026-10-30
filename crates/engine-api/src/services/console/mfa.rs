use axum::Json;
use chrono::Utc;

use crate::ApiState;
use crate::app::middleware::ConsoleAuthed;
use crate::dto::Empty;
use crate::dto::console::auth::{MfaCodeRequest, MfaEnrolmentResponse};
use crate::error::V2Error;
use crate::repositories::console_mutations::{FindConsoleUserMfaRow, SetConsoleUserMfaParams};
use crate::repositories::db::Queries;
use crate::services::deps::q;
use crate::utils::totp;

use super::RECOVERY_CODES;

pub(super) const MFA_CODE_REQUIRED: &str = "MFA code required.";

impl super::ConsoleService {
    pub(super) async fn verify_second_factor(
        &self,
        queries: &Queries,
        row: &FindConsoleUserMfaRow,
        code: &str,
    ) -> Result<(), V2Error> {
        let Some(secret) = row.mfa_secret.as_deref().map(String::from_utf8_lossy) else {
            if row.mfa_required.unwrap_or(false) {
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

    pub async fn enrol_mfa(
        &self,
        state: &ApiState,
        claims: &ConsoleAuthed,
    ) -> Result<Json<MfaEnrolmentResponse>, V2Error> {
        let queries = q(state).await?;
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
            uri: totp::otpauth_uri(&secret, "CYG Console", &row.username),
            secret,
            recovery_codes: codes,
        }))
    }

    pub async fn confirm_mfa(
        &self,
        state: &ApiState,
        claims: &ConsoleAuthed,
        body: MfaCodeRequest,
    ) -> Result<Json<Empty>, V2Error> {
        let queries = q(state).await?;
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

    pub async fn disable_mfa(
        &self,
        state: &ApiState,
        claims: &ConsoleAuthed,
        body: MfaCodeRequest,
    ) -> Result<Json<Empty>, V2Error> {
        let queries = q(state).await?;
        let row = self.console_row(&queries, claims).await?;
        self.verify_second_factor(&queries, &row, &body.code)
            .await?;
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
