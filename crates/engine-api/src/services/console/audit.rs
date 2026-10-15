//! `/v2/console/audit` and `/v2/console/acl` — the operator audit trail and the
//! named permission templates.

use axum::Json;

use crate::dto::console::Page;
use crate::dto::console::audit::{AclTemplateRow, AuditLogRow};
use crate::error::V2Error;
use crate::ApiState;

impl super::ConsoleService {
    pub async fn list_audit_logs(
        &self,
        state: &ApiState,
        page: Page,
    ) -> Result<Json<Vec<AuditLogRow>>, V2Error> {
        let rows = super::queries(state)?
            .list_audit_logs(page.limit(), page.offset())
            .await?;
        Ok(Json(rows.into_iter().map(AuditLogRow::from).collect()))
    }

    pub async fn list_acl_templates(
        &self,
        state: &ApiState,
    ) -> Result<Json<Vec<AclTemplateRow>>, V2Error> {
        let rows = super::queries(state)?.list_acl_templates().await?;
        Ok(Json(rows.into_iter().map(AclTemplateRow::from).collect()))
    }
}
