use axum::Json;
use uuid::Uuid;

use crate::app::middleware::ConsoleAuthed;
use crate::dto::Empty;
use crate::dto::console::audit::{
    AclTemplateRequest, AclTemplateRow, AuditLogQuery, AuditLogRow, AuditLogUsers,
};
use crate::error::V2Error;
use crate::repositories::console_mutations::{
    InsertAclTemplateParams, ListAuditLogsFilteredParams, UpdateAclTemplateParams,
};

impl super::ConsoleService {
    pub async fn list_audit_logs(
        &self,

        query: AuditLogQuery,
    ) -> Result<Json<Vec<AuditLogRow>>, V2Error> {
        let rows = self
            .plane
            .q()
            .await?
            .list_audit_logs_filtered(&ListAuditLogsFilteredParams {
                action: query.action.trim().to_owned(),
                resource: query.resource.trim().to_owned(),
                console_username: query.username.trim().to_owned(),
                create_time_start: query.after.unwrap_or_else(super::never),
                create_time_end: query.before.unwrap_or_else(crate::db::time::always),
                limit: query.limit(),
                offset: query.offset(),
            })
            .await?;
        Ok(Json(rows.into_iter().map(AuditLogRow::from).collect()))
    }

    pub async fn list_audit_log_users(&self) -> Result<Json<AuditLogUsers>, V2Error> {
        let usernames = self.plane.q().await?.audit_usernames().await?;
        Ok(Json(AuditLogUsers { usernames }))
    }

    pub async fn list_acl_templates(&self) -> Result<Json<Vec<AclTemplateRow>>, V2Error> {
        let rows = self.plane.q().await?.list_acl_templates().await?;
        Ok(Json(rows.into_iter().map(AclTemplateRow::from).collect()))
    }

    pub async fn create_acl_template(
        &self,

        claims: &ConsoleAuthed,
        body: AclTemplateRequest,
    ) -> Result<Json<AclTemplateRow>, V2Error> {
        claims.require_admin()?;
        let params = InsertAclTemplateParams {
            id: Uuid::new_v4(),
            name: template_name(&body.name)?,
            description: body.description.trim().to_owned(),
            acl: template_acl(body.acl)?,
        };
        let queries = self.plane.q().await?;
        queries
            .insert_acl_template(&params)
            .await
            .map_err(template_conflict)?;
        super::audit(
            &self.plane,
            claims,
            "acl.template.create",
            "acl_template",
            Some(params.id),
            &format!("Created ACL template {}.", params.name),
        )
        .await;
        Ok(Json(AclTemplateRow::from(
            queries.find_acl_template(params.id).await?,
        )))
    }

    /// Edit a template in place.
    ///
    /// With `expected` set, the update is one guarded statement: it writes only
    /// if the stored version is the one the caller saw, and the caller gets 409
    /// naming the version that won instead of silently overwriting it.
    pub async fn update_acl_template(
        &self,
        claims: &ConsoleAuthed,
        id: &str,
        body: AclTemplateRequest,
        expected: Option<i64>,
    ) -> Result<Json<AclTemplateRow>, V2Error> {
        claims.require_admin()?;
        let id = super::id(id, "template")?;
        let params = UpdateAclTemplateParams {
            id,
            name: template_name(&body.name)?,
            description: body.description.trim().to_owned(),
            acl: template_acl(body.acl)?,
        };
        let queries = self.plane.q().await?;
        let row = match expected {
            Some(expected) => match queries
                .update_acl_template_if_version(&params, expected)
                .await
                .map_err(template_conflict)?
            {
                Some(row) => row,
                None => {
                    let current = queries
                        .find_acl_template(id)
                        .await
                        .map_err(|_| V2Error::not_found("Template not found."))?;
                    return Err(crate::utils::precondition::stale(
                        current.version.unwrap_or(1),
                    ));
                }
            },
            None => {
                queries
                    .find_acl_template(id)
                    .await
                    .map_err(|_| V2Error::not_found("Template not found."))?;
                queries
                    .update_acl_template(&params)
                    .await
                    .map_err(template_conflict)?;
                queries.find_acl_template(id).await?
            }
        };
        super::audit(
            &self.plane,
            claims,
            "acl.template.update",
            "acl_template",
            Some(id),
            &format!("Edited ACL template {}.", params.name),
        )
        .await;
        Ok(Json(AclTemplateRow::from(row)))
    }

    pub async fn delete_acl_template(
        &self,

        claims: &ConsoleAuthed,
        id: &str,
    ) -> Result<Json<Empty>, V2Error> {
        claims.require_admin()?;
        let id = super::id(id, "template")?;
        let queries = self.plane.q().await?;
        let row = queries
            .find_acl_template(id)
            .await
            .map_err(|_| V2Error::not_found("Template not found."))?;
        queries.delete_acl_template(id).await?;
        super::audit(
            &self.plane,
            claims,
            "acl.template.delete",
            "acl_template",
            Some(id),
            &format!("Deleted ACL template {}.", row.name),
        )
        .await;
        Ok(Json(Empty {}))
    }
}

fn template_name(raw: &str) -> Result<String, V2Error> {
    let name = raw.trim();
    if name.is_empty() {
        return Err(V2Error::invalid_argument("A template needs a name."));
    }
    Ok(name.to_owned())
}

fn template_acl(raw: serde_json::Value) -> Result<serde_json::Value, V2Error> {
    if raw.is_null() {
        return Ok(serde_json::json!({}));
    }
    if !raw.is_object() {
        return Err(V2Error::invalid_argument(
            "A template's acl must be an object.",
        ));
    }
    Ok(raw)
}

fn template_conflict(e: crate::db::DbError) -> V2Error {
    if V2Error::is_unique_violation(&e, "template_name_uniq") {
        V2Error::already_exists("A template with that name already exists.")
    } else {
        V2Error::from(e)
    }
}
